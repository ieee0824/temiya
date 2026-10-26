//! Collector-independent heap handles, explicit roots, and tracing contracts.
//!
//! Handles never expose references directly. Heap access is scoped to a callback,
//! so a borrow cannot escape into interpreter state or survive a collection point.

use std::{
    any::Any,
    cell::RefCell,
    collections::HashMap,
    fmt,
    hash::{Hash, Hasher},
    marker::PhantomData,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

/// An erased heap identity passed to collector implementations.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RawHandle {
    heap: u64,
    slot: usize,
    generation: u64,
}

/// A typed, non-owning identity for a heap allocation.
pub struct Handle<T: Trace> {
    raw: RawHandle,
    marker: PhantomData<fn() -> T>,
}

impl<T: Trace> Handle<T> {
    /// Returns the collector-facing erased identity.
    #[must_use]
    pub fn erase(self) -> RawHandle {
        self.raw
    }
}

impl<T: Trace> Copy for Handle<T> {}

impl<T: Trace> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Trace> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

impl<T: Trace> Eq for Handle<T> {}

impl<T: Trace> Hash for Handle<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.raw.hash(state);
    }
}

impl<T: Trace> fmt::Debug for Handle<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("Handle").field(&self.raw).finish()
    }
}

/// A visitor used by [`Trace`] implementations to report outgoing heap edges.
pub struct Tracer<'a> {
    visit: &'a mut dyn FnMut(RawHandle),
}

impl Tracer<'_> {
    /// Reports a typed heap edge to the collector.
    pub fn mark<T: Trace>(&mut self, handle: Handle<T>) {
        (self.visit)(handle.erase());
    }
}

/// Describes the outgoing heap edges owned by a value.
///
/// Implementations must report every strong heap edge and must not retain the
/// supplied tracer. Values without heap edges can use an empty implementation.
pub trait Trace: Any {
    /// Visits every strong heap edge stored in this value.
    fn trace(&self, tracer: &mut Tracer<'_>);
}

trait Allocation: Trace {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Trace> Allocation for T {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

struct Slot {
    generation: u64,
    value: Box<dyn Allocation>,
}

type RootCounts = Rc<RefCell<HashMap<RawHandle, usize>>>;

/// An owning registration that keeps a handle reachable across collection points.
pub struct Root<T: Trace> {
    handle: Handle<T>,
    roots: RootCounts,
}

impl<T: Trace> Root<T> {
    /// Returns the non-owning handle protected by this root.
    #[must_use]
    pub fn handle(&self) -> Handle<T> {
        self.handle
    }
}

impl<T: Trace> Clone for Root<T> {
    fn clone(&self) -> Self {
        register_root(&self.roots, self.handle.raw);
        Self {
            handle: self.handle,
            roots: self.roots.clone(),
        }
    }
}

impl<T: Trace> Drop for Root<T> {
    fn drop(&mut self) {
        let mut roots = self.roots.borrow_mut();
        let count = roots.get_mut(&self.handle.raw).expect("registered root");
        *count -= 1;
        if *count == 0 {
            roots.remove(&self.handle.raw);
        }
    }
}

/// Storage boundary used by collectors and interpreter code.
///
/// The initial implementation is non-moving; its handles and trace contract do
/// not expose that choice, allowing later collectors to relocate allocations.
pub struct Heap {
    id: u64,
    slots: Vec<Slot>,
    roots: RootCounts,
}

impl Heap {
    /// Creates an empty heap with an independent handle namespace.
    #[must_use]
    pub fn new() -> Self {
        static NEXT_HEAP: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT_HEAP.fetch_add(1, Ordering::Relaxed),
            slots: Vec::new(),
            roots: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    /// Allocates and immediately roots a value.
    pub fn allocate<T: Trace>(&mut self, value: T) -> Root<T> {
        let handle = Handle {
            raw: RawHandle {
                heap: self.id,
                slot: self.slots.len(),
                generation: 0,
            },
            marker: PhantomData,
        };
        self.slots.push(Slot {
            generation: 0,
            value: Box::new(value),
        });
        register_root(&self.roots, handle.raw);
        Root {
            handle,
            roots: self.roots.clone(),
        }
    }

    /// Registers an existing live handle as an explicit root.
    pub fn root<T: Trace>(&self, handle: Handle<T>) -> Option<Root<T>> {
        self.slot(handle)?;
        register_root(&self.roots, handle.raw);
        Some(Root {
            handle,
            roots: self.roots.clone(),
        })
    }

    /// Provides scoped shared access if the handle is live and has the expected type.
    pub fn with<T: Trace, R>(
        &self,
        handle: Handle<T>,
        use_value: impl FnOnce(&T) -> R,
    ) -> Option<R> {
        self.slot(handle)
            .and_then(|slot| slot.value.as_any().downcast_ref::<T>())
            .map(use_value)
    }

    /// Provides scoped mutable access if the handle is live and has the expected type.
    pub fn with_mut<T: Trace, R>(
        &mut self,
        handle: Handle<T>,
        use_value: impl FnOnce(&mut T) -> R,
    ) -> Option<R> {
        self.slot_mut(handle)
            .and_then(|slot| slot.value.as_any_mut().downcast_mut::<T>())
            .map(use_value)
    }

    /// Returns the number of explicit root registrations for a handle.
    #[must_use]
    pub fn root_count<T: Trace>(&self, handle: Handle<T>) -> usize {
        self.roots.borrow().get(&handle.raw).copied().unwrap_or(0)
    }

    /// Traces each currently rooted allocation into a collector callback.
    pub fn trace_roots(&self, visit: &mut dyn FnMut(RawHandle)) {
        let roots = self.roots.borrow().keys().copied().collect::<Vec<_>>();
        for raw in roots {
            visit(raw);
            if let Some(slot) = self.raw_slot(raw) {
                slot.value.trace(&mut Tracer { visit });
            }
        }
    }

    fn slot<T: Trace>(&self, handle: Handle<T>) -> Option<&Slot> {
        self.raw_slot(handle.raw)
    }

    fn slot_mut<T: Trace>(&mut self, handle: Handle<T>) -> Option<&mut Slot> {
        if handle.raw.heap != self.id {
            return None;
        }
        self.slots
            .get_mut(handle.raw.slot)
            .filter(|slot| slot.generation == handle.raw.generation)
    }

    fn raw_slot(&self, handle: RawHandle) -> Option<&Slot> {
        if handle.heap != self.id {
            return None;
        }
        self.slots
            .get(handle.slot)
            .filter(|slot| slot.generation == handle.generation)
    }
}

impl Default for Heap {
    fn default() -> Self {
        Self::new()
    }
}

fn register_root(roots: &RootCounts, handle: RawHandle) {
    *roots.borrow_mut().entry(handle).or_default() += 1;
}
