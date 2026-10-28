//! Collector-independent heap handles, explicit roots, and tracing contracts.
//!
//! Handles never expose references directly. Heap access is scoped to a callback,
//! so a borrow cannot escape into interpreter state or survive a collection point.

use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::{HashSet, VecDeque},
    fmt,
    hash::{Hash, Hasher},
    marker::PhantomData,
    rc::{Rc, Weak},
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

    /// Creates a weak handle that does not contribute to reachability.
    #[must_use]
    pub fn downgrade(self) -> WeakHandle<T> {
        WeakHandle {
            raw: self.raw,
            marker: PhantomData,
        }
    }
}

/// A typed weak reference to an allocation.
pub struct WeakHandle<T: Trace> {
    raw: RawHandle,
    marker: PhantomData<fn() -> T>,
}

impl<T: Trace> WeakHandle<T> {
    /// Resolves this weak reference only while its target is still live.
    #[must_use]
    pub fn upgrade(self, heap: &Heap) -> Option<Handle<T>> {
        heap.raw_slot(self.raw)?;
        Some(Handle {
            raw: self.raw,
            marker: PhantomData,
        })
    }
}

impl<T: Trace> Copy for WeakHandle<T> {}
impl<T: Trace> Clone for WeakHandle<T> {
    fn clone(&self) -> Self {
        *self
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
    conditional: &'a mut dyn FnMut(RawHandle, RawHandle),
}

impl Tracer<'_> {
    /// Reports a typed heap edge to the collector.
    pub fn mark<T: Trace>(&mut self, handle: Handle<T>) {
        (self.visit)(handle.erase());
    }

    /// Reports an ephemeron owned by the allocation currently being traced.
    /// The value is retained only if both that owner and the key are reachable.
    /// This edge does not mark the key or visit the value until the key is
    /// reachable through another path.
    pub fn ephemeron<K: Trace, V: Trace>(&mut self, key: Handle<K>, value: Handle<V>) {
        (self.conditional)(key.erase(), value.erase());
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

// Runtime retirement releases the outer RefCell borrow before dropping host
// payloads. Standalone collection keeps its immediate-drop behavior.
pub(crate) struct RetiredAllocations(Vec<Box<dyn Allocation>>);

impl Drop for RetiredAllocations {
    fn drop(&mut self) {
        self.0.clear();
    }
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
    old: bool,
}

// Handles already carry a dense slot index. Root registration must not hash
// three identity words on every allocation, facade revival and final drop.
// The active list also keeps collection proportional to live roots rather
// than the heap's historical slot count.
#[derive(Default)]
struct RootTable {
    slots: Vec<Option<RootEntry>>,
    active: Vec<RawHandle>,
}
struct RootEntry {
    identity: RawHandle,
    count: usize,
    position: usize,
}
impl RootTable {
    fn register(&mut self, identity: RawHandle) {
        if self.slots.len() <= identity.slot {
            self.slots.resize_with(identity.slot + 1, || None);
        }
        if let Some(entry) = &mut self.slots[identity.slot] {
            if entry.count != 0 {
                assert_eq!(entry.identity, identity, "live root identity changed");
                entry.count += 1;
                return;
            }
        }
        self.slots[identity.slot] = Some(RootEntry {
            identity,
            count: 1,
            position: self.active.len(),
        });
        self.active.push(identity);
    }
    fn unregister(&mut self, identity: RawHandle) {
        let entry = self.slots[identity.slot].as_mut().expect("registered root");
        assert_eq!(entry.identity, identity, "root identity changed");
        assert_ne!(entry.count, 0, "root already unregistered");
        entry.count -= 1;
        if entry.count != 0 {
            return;
        }
        let position = entry.position;
        self.active.swap_remove(position);
        if let Some(moved) = self.active.get(position) {
            self.slots[moved.slot]
                .as_mut()
                .expect("active root")
                .position = position;
        }
    }
    fn count(&self, identity: RawHandle) -> usize {
        self.slots
            .get(identity.slot)
            .and_then(Option::as_ref)
            .filter(|entry| entry.identity == identity)
            .map_or(0, |entry| entry.count)
    }
}
type RootCounts = Rc<RefCell<RootTable>>;

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
        self.roots.borrow_mut().unregister(self.handle.raw);
    }
}

trait RegistrySweep {
    fn discover_dead(&self, live: &HashSet<RawHandle>);
    fn run_cleanup(&self);
}

struct RegistryInner<T> {
    entries: RefCell<Vec<(RawHandle, T)>>,
    pending: RefCell<VecDeque<T>>,
    callback: RefCell<Box<dyn FnMut(T)>>,
}

impl<T: 'static> RegistrySweep for RegistryInner<T> {
    fn discover_dead(&self, live: &HashSet<RawHandle>) {
        let mut entries = self.entries.borrow_mut();
        let mut pending = self.pending.borrow_mut();
        let mut index = 0;
        while index < entries.len() {
            if live.contains(&entries[index].0) {
                index += 1;
            } else {
                let (_, held) = entries.remove(index);
                pending.push_back(held);
            }
        }
    }

    fn run_cleanup(&self) {
        while let Some(held) = self.pending.borrow_mut().pop_front() {
            (self.callback.borrow_mut())(held);
        }
    }
}

/// Registers held values for ordered cleanup after weak targets become unreachable.
pub struct FinalizationRegistry<T: 'static> {
    inner: Rc<RegistryInner<T>>,
}

impl<T: 'static> FinalizationRegistry<T> {
    /// Creates a registry attached to a heap's cleanup-job queue.
    pub fn new(heap: &mut Heap, callback: impl FnMut(T) + 'static) -> Self {
        let inner = Rc::new(RegistryInner {
            entries: RefCell::new(Vec::new()),
            pending: RefCell::new(VecDeque::new()),
            callback: RefCell::new(Box::new(callback)),
        });
        let erased: Rc<dyn RegistrySweep> = inner.clone();
        heap.registries.push(Rc::downgrade(&erased));
        Self { inner }
    }

    /// Associates a held value with a weak target without retaining that target.
    pub fn register<U: Trace>(&self, target: Handle<U>, held: T) {
        self.inner.entries.borrow_mut().push((target.raw, held));
    }

    /// Returns the number of cleanup records waiting to run.
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.inner.pending.borrow().len()
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
    remembered: HashSet<RawHandle>,
    ephemerons: Vec<(RawHandle, RawHandle)>,
    registries: Vec<Weak<dyn RegistrySweep>>,
    collections: usize,
    reclaimed_total: usize,
    peak_live: usize,
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

/// A deterministic snapshot for heap diagnostics and leak investigation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HeapDiagnostics {
    /// Currently live allocations.
    pub live: usize,
    /// Total slot capacity, including reusable slots.
    pub slots: usize,
    /// Slots available for reuse.
    pub free: usize,
    /// Distinct explicitly rooted allocations.
    pub rooted: usize,
    /// Old owners currently in the remembered set.
    pub remembered: usize,
    /// Collections completed by this heap.
    pub collections: usize,
    /// Allocations reclaimed over the heap lifetime.
    pub reclaimed_total: usize,
    /// Highest observed live-allocation count.
    pub peak_live: usize,
}

/// Structural corruption detected by [`Heap::verify`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeapVerificationError {
    /// A live value traces an edge that cannot be resolved.
    DanglingEdge { owner: RawHandle, edge: RawHandle },
    /// A root registration cannot be resolved.
    DanglingRoot(RawHandle),
    /// A remembered owner is not a live old allocation.
    InvalidRememberedOwner(RawHandle),
    /// The free list names a live, missing, or duplicated slot.
    InvalidFreeSlot(usize),
}

impl fmt::Display for HeapVerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "heap verification failed: {self:?}")
    }
}

impl std::error::Error for HeapVerificationError {}

/// Allocation failure returned after collection cannot satisfy a heap limit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutOfMemory;

impl fmt::Display for OutOfMemory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("garbage-collected heap allocation limit reached")
    }
}

impl std::error::Error for OutOfMemory {}

/// A write-barrier validation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteBarrierError {
    /// The owner handle is not live in this heap.
    InvalidOwner,
    /// The target handle is not live in this heap.
    InvalidTarget,
    /// An old allocation contains a young edge but is absent from the remembered set.
    MissingBarrier(RawHandle),
}

impl fmt::Display for WriteBarrierError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOwner => formatter.write_str("write barrier owner is not live"),
            Self::InvalidTarget => formatter.write_str("write barrier target is not live"),
            Self::MissingBarrier(_) => {
                formatter.write_str("old-to-young edge lacks a write barrier")
            }
        }
    }
}

impl std::error::Error for WriteBarrierError {}

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
            roots: Rc::new(RefCell::new(RootTable::default())),
            options,
            allocations_since_collection: 0,
            remembered: HashSet::new(),
            ephemerons: Vec::new(),
            registries: Vec::new(),
            collections: 0,
            reclaimed_total: 0,
            peak_live: 0,
        }
    }

    /// Creates a heap that performs a full collection before every allocation.
    #[must_use]
    pub fn stress() -> Self {
        Self::with_options(HeapOptions {
            allocation_threshold: 0,
            max_allocations: None,
        })
    }

    /// Allocates and immediately roots a value.
    pub fn allocate<T: Trace>(&mut self, value: T) -> Root<T> {
        self.try_allocate(value)
            .expect("garbage-collected heap allocation failed")
    }

    /// Allocates and roots a value, collecting first when configured limits require it.
    pub fn try_allocate<T: Trace>(&mut self, value: T) -> Result<Root<T>, OutOfMemory> {
        if self.allocations_since_collection >= self.options.allocation_threshold {
            self.collect_with_retire(drop, Some(&value));
        }
        if self
            .options
            .max_allocations
            .is_some_and(|limit| self.live_allocations() >= limit)
        {
            self.collect_with_retire(drop, Some(&value));
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
                old: false,
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
        self.slots[slot_index].old = false;
        self.allocations_since_collection += 1;
        self.peak_live = self.peak_live.max(self.live_allocations());
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

    /// Records a possible old-to-young edge before or after mutating its owner.
    ///
    /// Embedders must call this whenever an old allocation gains a strong edge.
    pub fn write_barrier<T: Trace, U: Trace>(
        &mut self,
        owner: Handle<T>,
        target: Handle<U>,
    ) -> Result<(), WriteBarrierError> {
        let owner_slot = self
            .raw_slot(owner.raw)
            .ok_or(WriteBarrierError::InvalidOwner)?;
        self.raw_slot(target.raw)
            .ok_or(WriteBarrierError::InvalidTarget)?;
        if owner_slot.old && self.is_young(target.raw) {
            self.remembered.insert(owner.raw);
        }
        Ok(())
    }

    /// Validates a payload's edges before publishing it and remembers its owner.
    pub(crate) fn record_traced_edges<T: Trace, U: Trace + ?Sized>(
        &mut self,
        owner: Handle<T>,
        edges: &U,
    ) -> Result<(), WriteBarrierError> {
        let old = self
            .raw_slot(owner.raw)
            .ok_or(WriteBarrierError::InvalidOwner)?
            .old;
        let invalid = Cell::new(false);
        let young = Cell::new(false);
        let check = |edge| match self.raw_slot(edge) {
            Some(slot) => young.set(young.get() || !slot.old),
            None => invalid.set(true),
        };
        edges.trace(&mut Tracer {
            visit: &mut |edge| check(edge),
            conditional: &mut |key, value| {
                check(key);
                check(value);
            },
        });
        if invalid.get() {
            return Err(WriteBarrierError::InvalidTarget);
        }
        if old && young.get() {
            self.remembered.insert(owner.raw);
        }
        Ok(())
    }

    /// Verifies that every observed old-to-young edge has a remembered owner.
    pub fn verify_write_barriers(&self) -> Result<(), WriteBarrierError> {
        for (index, slot) in self.slots.iter().enumerate() {
            let Some(value) = &slot.value else { continue };
            if !slot.old {
                continue;
            }
            let owner = RawHandle {
                heap: self.id,
                slot: index,
                generation: slot.generation,
            };
            let has_young_edge = Cell::new(false);
            value.trace(&mut Tracer {
                visit: &mut |edge| has_young_edge.set(has_young_edge.get() || self.is_young(edge)),
                conditional: &mut |key, value| {
                    has_young_edge
                        .set(has_young_edge.get() || self.is_young(key) || self.is_young(value))
                },
            });
            if has_young_edge.get() && !self.remembered.contains(&owner) {
                return Err(WriteBarrierError::MissingBarrier(owner));
            }
        }
        Ok(())
    }

    /// Adds an ephemeron whose value is retained only when its key is reachable.
    pub fn add_ephemeron<K: Trace, V: Trace>(
        &mut self,
        key: Handle<K>,
        value: Handle<V>,
    ) -> Result<(), WriteBarrierError> {
        self.raw_slot(key.raw)
            .ok_or(WriteBarrierError::InvalidOwner)?;
        self.raw_slot(value.raw)
            .ok_or(WriteBarrierError::InvalidTarget)?;
        self.ephemerons.push((key.raw, value.raw));
        Ok(())
    }

    /// Runs queued finalization cleanup callbacks in registry and registration order.
    pub fn run_cleanup_jobs(&mut self) {
        self.registries.retain(|registry| {
            if let Some(registry) = registry.upgrade() {
                registry.run_cleanup();
                true
            } else {
                false
            }
        });
    }

    /// Returns the number of explicit root registrations for a handle.
    #[must_use]
    pub fn root_count<T: Trace>(&self, handle: Handle<T>) -> usize {
        self.roots.borrow().count(handle.raw)
    }

    /// Traces each currently rooted allocation into a collector callback.
    pub fn trace_roots(&self, visit: &mut dyn FnMut(RawHandle)) {
        let roots = self.roots.borrow().active.clone();
        for raw in roots {
            visit(raw);
            if let Some(slot) = self.raw_slot(raw) {
                if let Some(value) = &slot.value {
                    value.trace(&mut Tracer {
                        visit,
                        conditional: &mut |_, _| {},
                    });
                }
            }
        }
    }

    /// Performs a complete stop-the-world mark-and-sweep collection.
    pub fn collect(&mut self) -> CollectionStats {
        self.collect_with_retire(drop, None)
    }

    pub(crate) fn collect_retiring(&mut self) -> (CollectionStats, RetiredAllocations) {
        let mut retired = RetiredAllocations(Vec::new());
        let stats = self.collect_with_retire(|value| retired.0.push(value), None);
        (stats, retired)
    }

    pub(crate) fn collect_retiring_with(
        &mut self,
        incoming: &impl Trace,
    ) -> (CollectionStats, RetiredAllocations) {
        let mut retired = RetiredAllocations(Vec::new());
        let stats = self.collect_with_retire(|value| retired.0.push(value), Some(incoming));
        (stats, retired)
    }

    fn collect_with_retire(
        &mut self,
        mut retire: impl FnMut(Box<dyn Allocation>),
        incoming: Option<&dyn Trace>,
    ) -> CollectionStats {
        let mut marked = HashSet::new();
        let mut traced_ephemerons = Vec::new();
        let mut queue = VecDeque::from(self.roots.borrow().active.clone());
        if let Some(incoming) = incoming {
            incoming.trace(&mut Tracer {
                visit: &mut |edge| queue.push_back(edge),
                conditional: &mut |key, value| traced_ephemerons.push((key, value)),
            });
        }
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
                conditional: &mut |key, value| traced_ephemerons.push((key, value)),
            });
        }
        loop {
            let mut added = false;
            for &(key, value) in self.ephemerons.iter().chain(&traced_ephemerons) {
                if marked.contains(&key) && !marked.contains(&value) {
                    queue.push_back(value);
                    added = true;
                }
            }
            while let Some(raw) = queue.pop_front() {
                if !marked.insert(raw) {
                    continue;
                }
                let Some(value) = self.raw_slot(raw).and_then(|slot| slot.value.as_ref()) else {
                    continue;
                };
                value.trace(&mut Tracer {
                    visit: &mut |edge| queue.push_back(edge),
                    conditional: &mut |key, value| traced_ephemerons.push((key, value)),
                });
            }
            if !added {
                break;
            }
        }

        self.registries
            .retain(|registry| registry.strong_count() > 0);
        for registry in self.registries.iter().filter_map(Weak::upgrade) {
            registry.discover_dead(&marked);
        }

        let mut reclaimed = 0;
        for (index, slot) in self.slots.iter_mut().enumerate() {
            let raw = RawHandle {
                heap: self.id,
                slot: index,
                generation: slot.generation,
            };
            if slot.value.is_some() && !marked.contains(&raw) {
                let value = slot.value.take().expect("occupied dead slot");
                retire(value);
                self.free.push(index);
                reclaimed += 1;
            } else if slot.value.is_some() {
                slot.old = true;
            }
        }
        self.remembered.clear();
        self.ephemerons
            .retain(|(key, value)| marked.contains(key) && marked.contains(value));
        self.allocations_since_collection = 0;
        self.collections += 1;
        self.reclaimed_total += reclaimed;
        CollectionStats {
            retained: self.live_allocations(),
            reclaimed,
        }
    }

    /// Collects unreachable young allocations while leaving the old generation intact.
    pub fn collect_minor(&mut self) -> CollectionStats {
        let mut marked = HashSet::new();
        let mut traced_ephemerons = Vec::new();
        let mut queue = VecDeque::new();
        let roots = self.roots.borrow().active.clone();
        let seeds = roots
            .iter()
            .copied()
            .chain(self.remembered.iter().copied())
            .collect::<HashSet<_>>();
        for raw in seeds {
            if self.is_young(raw) {
                queue.push_back(raw);
            } else if let Some(value) = self.raw_slot(raw).and_then(|slot| slot.value.as_ref()) {
                value.trace(&mut Tracer {
                    conditional: &mut |key, value| traced_ephemerons.push((key, value)),
                    visit: &mut |edge| {
                        if self.is_young(edge) {
                            queue.push_back(edge);
                        }
                    },
                });
            }
        }
        while let Some(raw) = queue.pop_front() {
            if !marked.insert(raw) {
                continue;
            }
            let Some(value) = self.raw_slot(raw).and_then(|slot| slot.value.as_ref()) else {
                continue;
            };
            value.trace(&mut Tracer {
                conditional: &mut |key, value| traced_ephemerons.push((key, value)),
                visit: &mut |edge| {
                    if self.is_young(edge) {
                        queue.push_back(edge);
                    }
                },
            });
        }
        loop {
            let mut added = false;
            for &(key, value) in self.ephemerons.iter().chain(&traced_ephemerons) {
                let key_reachable = self
                    .raw_slot(key)
                    .is_some_and(|slot| slot.old || marked.contains(&key));
                if key_reachable && self.is_young(value) && !marked.contains(&value) {
                    queue.push_back(value);
                    added = true;
                }
            }
            while let Some(raw) = queue.pop_front() {
                if !marked.insert(raw) {
                    continue;
                }
                let Some(value) = self.raw_slot(raw).and_then(|slot| slot.value.as_ref()) else {
                    continue;
                };
                value.trace(&mut Tracer {
                    conditional: &mut |key, value| traced_ephemerons.push((key, value)),
                    visit: &mut |edge| {
                        if self.is_young(edge) {
                            queue.push_back(edge);
                        }
                    },
                });
            }
            if !added {
                break;
            }
        }

        let mut reclaimed = 0;
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if slot.old || slot.value.is_none() {
                continue;
            }
            let raw = RawHandle {
                heap: self.id,
                slot: index,
                generation: slot.generation,
            };
            if marked.contains(&raw) {
                slot.old = true;
            } else {
                slot.value = None;
                self.free.push(index);
                reclaimed += 1;
            }
        }
        self.remembered.clear();
        self.allocations_since_collection = 0;
        self.collections += 1;
        self.reclaimed_total += reclaimed;
        CollectionStats {
            retained: self.live_allocations(),
            reclaimed,
        }
    }

    /// Returns the number of allocations currently present in the heap.
    #[must_use]
    pub fn live_allocations(&self) -> usize {
        // Every vacant slot appears exactly once in the free list. This is
        // also called on each allocation to update peak_live; scanning all
        // slots here makes allocation cost grow with the heap high-water mark.
        self.slots.len() - self.free.len()
    }

    /// Returns current and cumulative heap statistics without triggering collection.
    #[must_use]
    pub fn diagnostics(&self) -> HeapDiagnostics {
        HeapDiagnostics {
            live: self.live_allocations(),
            slots: self.slots.len(),
            free: self.free.len(),
            rooted: self.roots.borrow().active.len(),
            remembered: self.remembered.len(),
            collections: self.collections,
            reclaimed_total: self.reclaimed_total,
            peak_live: self.peak_live,
        }
    }

    /// Verifies roots, traced edges, remembered owners, and the free list.
    pub fn verify(&self) -> Result<(), HeapVerificationError> {
        for raw in self.roots.borrow().active.iter().copied() {
            if self.raw_slot(raw).is_none() {
                return Err(HeapVerificationError::DanglingRoot(raw));
            }
        }
        for &raw in &self.remembered {
            if !self.raw_slot(raw).is_some_and(|slot| slot.old) {
                return Err(HeapVerificationError::InvalidRememberedOwner(raw));
            }
        }
        let mut free = HashSet::new();
        for &index in &self.free {
            if !free.insert(index)
                || self
                    .slots
                    .get(index)
                    .is_none_or(|slot| slot.value.is_some())
            {
                return Err(HeapVerificationError::InvalidFreeSlot(index));
            }
        }
        for (index, slot) in self.slots.iter().enumerate() {
            let Some(value) = &slot.value else { continue };
            let owner = RawHandle {
                heap: self.id,
                slot: index,
                generation: slot.generation,
            };
            let dangling = Cell::new(None);
            value.trace(&mut Tracer {
                visit: &mut |edge| {
                    if dangling.get().is_none() && self.raw_slot(edge).is_none() {
                        dangling.set(Some(edge));
                    }
                },
                conditional: &mut |key, value| {
                    // A dead weak key may leave a stale entry until its owner's
                    // next cleanup. A live key must still have a live value.
                    if dangling.get().is_none()
                        && self.raw_slot(key).is_some()
                        && self.raw_slot(value).is_none()
                    {
                        dangling.set(Some(value));
                    }
                },
            });
            if let Some(edge) = dangling.get() {
                return Err(HeapVerificationError::DanglingEdge { owner, edge });
            }
        }
        Ok(())
    }

    fn is_young(&self, handle: RawHandle) -> bool {
        self.raw_slot(handle).is_some_and(|slot| !slot.old)
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
    roots.borrow_mut().register(handle);
}
