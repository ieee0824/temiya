//! Collector-independent heap handles, explicit roots, and tracing contracts.
//!
//! Handles never expose references directly. Heap access is scoped to a callback,
//! so a borrow cannot escape into interpreter state or survive a collection point.

use std::{
    any::Any,
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
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
    value: Option<Box<dyn Allocation>>,
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
    free: Vec<usize>,
    roots: RootCounts,
    options: HeapOptions,
    allocations_since_collection: usize,
}

/// Limits controlling automatic collection and allocation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HeapOptions {
    /// Number of allocations between automatic collection attempts.
    pub allocation_threshold: usize,
    /// Maximum simultaneously live allocations, or no limit when `None`.
    pub max_allocations: Option<usize>,
}

impl Default for HeapOptions {
    fn default() -> Self {
        Self {
            allocation_threshold: 1_024,
            max_allocations: None,
        }
    }
}

/// Statistics produced by one stop-the-world collection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CollectionStats {
    /// Allocations reachable from explicit roots.
    pub retained: usize,
    /// Unreachable allocations reclaimed by this collection.
    pub reclaimed: usize,
}

/// Allocation failure returned after collection cannot satisfy a heap limit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutOfMemory;

impl fmt::Display for OutOfMemory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("garbage-collected heap allocation limit reached")
    }
}

impl std::error::Error for OutOfMemory {}

impl Heap {
    /// Creates an empty heap with an independent handle namespace.
    #[must_use]
    pub fn new() -> Self {
        Self::with_options(HeapOptions::default())
    }

    /// Creates an empty heap with explicit collector limits.
    #[must_use]
    pub fn with_options(options: HeapOptions) -> Self {
        static NEXT_HEAP: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT_HEAP.fetch_add(1, Ordering::Relaxed),
            slots: Vec::new(),
            free: Vec::new(),
            roots: Rc::new(RefCell::new(HashMap::new())),
            options,
            allocations_since_collection: 0,
        }
    }

    /// Allocates and immediately roots a value.
    pub fn allocate<T: Trace>(&mut self, value: T) -> Root<T> {
        self.try_allocate(value)
            .expect("garbage-collected heap allocation failed")
    }

    /// Allocates and roots a value, collecting first when configured limits require it.
    pub fn try_allocate<T: Trace>(&mut self, value: T) -> Result<Root<T>, OutOfMemory> {
        if self.allocations_since_collection >= self.options.allocation_threshold {
            self.collect();
        }
        if self
            .options
            .max_allocations
            .is_some_and(|limit| self.live_allocations() >= limit)
        {
            self.collect();
            if self
                .options
                .max_allocations
                .is_some_and(|limit| self.live_allocations() >= limit)
            {
                return Err(OutOfMemory);
            }
        }
        let (slot_index, generation) = if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index];
            slot.generation = slot.generation.wrapping_add(1);
            (index, slot.generation)
        } else {
            self.slots.push(Slot {
                generation: 0,
                value: None,
            });
            (self.slots.len() - 1, 0)
        };
        let handle = Handle {
            raw: RawHandle {
                heap: self.id,
                slot: slot_index,
                generation,
            },
            marker: PhantomData,
        };
        self.slots[slot_index].value = Some(Box::new(value));
        self.allocations_since_collection += 1;
        register_root(&self.roots, handle.raw);
        Ok(Root {
            handle,
            roots: self.roots.clone(),
        })
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
            .and_then(|slot| slot.value.as_ref()?.as_any().downcast_ref::<T>())
            .map(use_value)
    }

    /// Provides scoped mutable access if the handle is live and has the expected type.
    pub fn with_mut<T: Trace, R>(
        &mut self,
        handle: Handle<T>,
        use_value: impl FnOnce(&mut T) -> R,
    ) -> Option<R> {
        self.slot_mut(handle)
            .and_then(|slot| slot.value.as_mut()?.as_any_mut().downcast_mut::<T>())
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
                if let Some(value) = &slot.value {
                    value.trace(&mut Tracer { visit });
                }
            }
        }
    }

    /// Performs a complete stop-the-world mark-and-sweep collection.
    pub fn collect(&mut self) -> CollectionStats {
        let mut marked = HashSet::new();
        let mut queue = self.roots.borrow().keys().copied().collect::<VecDeque<_>>();
        while let Some(raw) = queue.pop_front() {
            if !marked.insert(raw) {
                continue;
            }
            let Some(slot) = self.raw_slot(raw) else {
                continue;
            };
            let Some(value) = &slot.value else { continue };
            value.trace(&mut Tracer {
                visit: &mut |edge| queue.push_back(edge),
            });
        }

        let mut reclaimed = 0;
        for (index, slot) in self.slots.iter_mut().enumerate() {
            let raw = RawHandle {
                heap: self.id,
                slot: index,
                generation: slot.generation,
            };
            if slot.value.is_some() && !marked.contains(&raw) {
                slot.value = None;
                self.free.push(index);
                reclaimed += 1;
            }
        }
        self.allocations_since_collection = 0;
        CollectionStats {
            retained: self.live_allocations(),
            reclaimed,
        }
    }

    /// Returns the number of allocations currently present in the heap.
    #[must_use]
    pub fn live_allocations(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.value.is_some())
            .count()
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
            .filter(|slot| slot.generation == handle.raw.generation && slot.value.is_some())
    }

    fn raw_slot(&self, handle: RawHandle) -> Option<&Slot> {
        if handle.heap != self.id {
            return None;
        }
        self.slots
            .get(handle.slot)
            .filter(|slot| slot.generation == handle.generation && slot.value.is_some())
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
