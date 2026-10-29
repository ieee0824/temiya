//! FIFO Promise-job queue owned by a Realm.

use super::{Runtime, RuntimeError, RuntimeResult, stored::StoredOwner};
use crate::gc::{Handle, Root, Trace, Tracer};
use std::{
    cell::RefCell,
    collections::VecDeque,
    fmt,
    ops::Deref,
    rc::{Rc, Weak},
};

type UnhandledHook = Rc<dyn Fn(RuntimeError)>;

/// Internal jobs must report all managed edges and store no owning public facades.
pub(super) trait TracedJob: Trace {
    fn run(&self, queue: &JobQueue) -> RuntimeResult<()>;
}

// Host callbacks retain external public roots explicitly. Their Rust captures
// are opaque and follow the same cycle-release contract as other host hooks.
struct HostJob<F>(RefCell<Option<F>>);
impl<F: 'static> Trace for HostJob<F> {
    fn trace(&self, _: &mut Tracer<'_>) {}
}
impl<F: FnOnce() -> RuntimeResult<()> + 'static> TracedJob for HostJob<F> {
    fn run(&self, _: &JobQueue) -> RuntimeResult<()> {
        let callback = self
            .0
            .borrow_mut()
            .take()
            .ok_or_else(|| RuntimeError::internal_error("job already executed"))?;
        callback()
    }
}

#[derive(Default)]
pub(super) struct JobQueueData {
    pub(super) facade: Weak<JobQueueRoot>,
    jobs: VecDeque<Rc<dyn TracedJob>>,
    active: Vec<Rc<dyn TracedJob>>,
    draining: bool,
    // Ordinary synchronous Realms need neither a rejection hook nor checkpoint
    // storage. Keep both out of the always-allocated queue record.
    signals: Option<Box<JobQueueSignals>>,
}
#[derive(Default)]
struct JobQueueSignals {
    checkpoints: VecDeque<Rc<dyn TracedJob>>,
    unhandled_hook: Option<UnhandledHook>,
}
impl JobQueueData {
    fn signals_mut(&mut self) -> &mut JobQueueSignals {
        self.signals.get_or_insert_with(Box::default)
    }
}
pub(super) type ManagedJobQueue = RefCell<JobQueueData>;
impl Trace for RefCell<JobQueueData> {
    fn trace(&self, tracer: &mut Tracer<'_>) {
        let data = self.borrow();
        for job in data.jobs.iter().chain(&data.active) {
            job.trace(tracer);
        }
        if let Some(signals) = &data.signals {
            for job in &signals.checkpoints {
                job.trace(tracer);
            }
        }
    }
}
pub(super) struct JobQueueRoot {
    pub(super) root: Root<ManagedJobQueue>,
    pub(super) data: Rc<RefCell<JobQueueData>>,
    pub(super) runtime: Runtime,
}
#[derive(Clone)]
pub(super) struct JobQueueReference(pub(super) Rc<JobQueueRoot>);
impl Deref for JobQueueReference {
    type Target = RefCell<JobQueueData>;
    fn deref(&self) -> &Self::Target {
        &self.0.data
    }
}

/// A Realm-local FIFO queue for Promise reactions and thenable jobs.
#[derive(Clone)]
pub struct JobQueue(pub(super) JobQueueReference);
impl StoredOwner for JobQueue {
    type Allocation = ManagedJobQueue;
    fn runtime(&self) -> Option<&Runtime> {
        Some(&self.0.0.runtime)
    }
    fn handle(&self) -> Option<Handle<ManagedJobQueue>> {
        Some(self.0.0.root.handle())
    }
}

#[derive(Clone)]
pub(super) struct StoredJobQueue(pub(super) Handle<ManagedJobQueue>);
impl StoredJobQueue {
    pub(super) fn initial(runtime: Option<&Runtime>, value: &JobQueue) -> RuntimeResult<Self> {
        match (runtime, value.runtime()) {
            (Some(left), Some(right)) if left.same_heap(right) => {
                Ok(Self(value.handle().expect("managed job_queue")))
            }
            _ => Err(RuntimeError::type_error(
                "Job Queue allocation context mismatch",
            )),
        }
    }
    pub(super) fn store(owner: &impl StoredOwner, value: &JobQueue) -> RuntimeResult<Self> {
        let stored = Self::initial(owner.runtime(), value)?;
        owner
            .runtime()
            .expect("validated allocation context")
            .record_object_edge(owner.handle().expect("managed owner"), stored.0)?;
        Ok(stored)
    }
    pub(super) fn load(&self, owner: &impl StoredOwner) -> RuntimeResult<JobQueue> {
        owner
            .runtime()
            .ok_or_else(|| RuntimeError::internal_error("Job Queue allocation context mismatch"))?
            .root_job_queue(self.0)
            .ok_or_else(|| RuntimeError::internal_error("stale job_queue edge"))
    }
    pub(super) fn trace(&self, tracer: &mut Tracer<'_>) {
        tracer.mark(self.0);
    }
}
impl fmt::Debug for JobQueue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JobQueue")
            .field("len", &self.len())
            .finish_non_exhaustive()
    }
}

struct ActiveJob {
    queue: JobQueue,
    job: Rc<dyn TracedJob>,
}
impl Drop for ActiveJob {
    fn drop(&mut self) {
        let retired = {
            let mut data = self.queue.0.borrow_mut();
            let index = data
                .active
                .iter()
                .rposition(|job| Rc::ptr_eq(job, &self.job))
                .expect("active job");
            data.active.swap_remove(index)
        };
        drop(retired);
    }
}
struct Draining(JobQueue);
impl Drop for Draining {
    fn drop(&mut self) {
        self.0.0.borrow_mut().draining = false;
    }
}

impl JobQueue {
    /// Creates an empty queue.
    #[must_use]
    pub fn new(runtime: &super::AllocationContext) -> Self {
        runtime.allocate_job_queue(JobQueueData::default())
    }

    /// Enqueues one host job at the tail of the queue.
    /// Captured public roots follow the host callback cycle-release contract.
    pub fn enqueue(&self, job: impl FnOnce() -> RuntimeResult<()> + 'static) {
        self.enqueue_traced(HostJob(RefCell::new(Some(job))))
            .expect("host job has no managed edges");
    }

    pub(super) fn enqueue_traced(&self, job: impl TracedJob + 'static) -> RuntimeResult<()> {
        self.enqueue_into(job, false)
    }

    pub(super) fn enqueue_checkpoint(&self, job: impl TracedJob + 'static) -> RuntimeResult<()> {
        self.enqueue_into(job, true)
    }

    fn enqueue_into(&self, job: impl TracedJob + 'static, checkpoint: bool) -> RuntimeResult<()> {
        if let Some(runtime) = self.runtime() {
            runtime.record_job_edges(self.handle().expect("managed queue"), &job)?;
        }
        let mut data = self.0.borrow_mut();
        if checkpoint {
            data.signals_mut().checkpoints.push_back(Rc::new(job));
        } else {
            data.jobs.push_back(Rc::new(job));
        }
        Ok(())
    }

    /// Returns the number of jobs waiting to run.
    #[must_use]
    pub fn len(&self) -> usize {
        let data = self.0.borrow();
        data.jobs.len()
            + data
                .signals
                .as_ref()
                .map_or(0, |signals| signals.checkpoints.len())
    }
    /// Returns whether the queue has no pending jobs.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Runs one queued job, returning whether a job was available.
    pub fn run_one(&self) -> RuntimeResult<bool> {
        let job = {
            let mut data = self.0.borrow_mut();
            // Rejection checks observe the end of the microtask checkpoint,
            // after queued reactions have had a chance to attach a handler.
            let Some(job) = data.jobs.pop_front().or_else(|| {
                data.signals
                    .as_mut()
                    .and_then(|signals| signals.checkpoints.pop_front())
            }) else {
                return Ok(false);
            };
            data.active.push(job.clone());
            job
        };
        // The queue remains rooted through this guard, and reports this job's
        // edges even after dequeue, including nested jobs and abrupt exits.
        let active = ActiveJob {
            queue: self.clone(),
            job,
        };
        active.job.run(self)?;
        Ok(true)
    }

    /// Drains the queue to a microtask checkpoint, including jobs enqueued by jobs.
    pub fn drain(&self) -> RuntimeResult<usize> {
        if self.0.borrow().draining {
            return Err(RuntimeError::type_error("Job Queue is already draining"));
        }
        self.0.borrow_mut().draining = true;
        let _draining = Draining(self.clone());
        let mut count = 0;
        while self.run_one()? {
            count += 1;
        }
        Ok(count)
    }

    /// Installs the host callback used for still-unhandled rejections.
    pub fn set_unhandled_rejection_hook(&self, hook: impl Fn(RuntimeError) + 'static) {
        let retired = self
            .0
            .borrow_mut()
            .signals_mut()
            .unhandled_hook
            .replace(Rc::new(hook));
        drop(retired);
    }

    pub(super) fn report_unhandled(&self, error: RuntimeError) {
        let hook = self
            .0
            .borrow()
            .signals
            .as_ref()
            .and_then(|signals| signals.unhandled_hook.clone());
        if let Some(hook) = hook {
            hook(error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{Object, Realm, Value, stored::StoredObject};

    #[test]
    fn allocation_safepoints_trace_unpublished_queue_payloads() {
        for checkpoint in [false, true] {
            let runtime = Runtime::new();
            runtime.set_allocation_collection_interval(0);
            let object = Object::new_in(&runtime);
            object
                .create_data_property("answer".into(), Value::Number(42.0))
                .unwrap();
            let weak = object.downgrade();
            let mut incoming = JobQueueData::default();
            let job = Rc::new(ReadJob {
                capture: StoredObject::initial(Some(&runtime), &object).unwrap(),
                nested: None,
                fail: false,
            });
            if checkpoint {
                incoming.signals_mut().checkpoints.push_back(job);
            } else {
                incoming.jobs.push_back(job);
            }
            drop(object);
            assert_eq!(runtime.diagnostics().rooted, 0);
            let queue = runtime.allocate_job_queue(incoming);
            assert!(weak.upgrade().is_some());
            assert!(queue.run_one().unwrap());
            drop(queue);
            assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
            assert!(weak.upgrade().is_none());
        }
    }

    struct ReadJob {
        capture: StoredObject,
        nested: Option<StoredObject>,
        fail: bool,
    }
    impl Trace for ReadJob {
        fn trace(&self, tracer: &mut Tracer<'_>) {
            self.capture.trace(tracer);
            if let Some(nested) = &self.nested {
                nested.trace(tracer);
            }
        }
    }
    impl TracedJob for ReadJob {
        fn run(&self, queue: &JobQueue) -> RuntimeResult<()> {
            queue.runtime().unwrap().collect_migration_graph().unwrap();
            let check = || {
                let object = self.capture.load(queue).unwrap();
                assert!(
                    object
                        .get(&"answer".into())
                        .unwrap()
                        .same_value(&Value::Number(42.0))
                );
            };
            check();
            if let Some(nested) = &self.nested {
                queue.enqueue_traced(Self {
                    capture: nested.clone(),
                    nested: None,
                    fail: false,
                })?;
                assert!(queue.run_one()?);
                check();
            }
            if self.fail {
                Err(RuntimeError::type_error("job failed"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn traced_jobs_retain_captures_after_dequeue_nested_runs_and_errors() {
        for fail in [false, true] {
            let runtime = Runtime::new();
            let queue = JobQueue::new(&runtime);
            runtime.collect_migration_graph().unwrap();
            let first = Object::new_in(&runtime);
            let second = Object::new_in(&runtime);
            for object in [&first, &second] {
                object
                    .create_data_property("answer".into(), Value::Number(42.0))
                    .unwrap();
                object
                    .create_data_property("self".into(), Value::Object(object.clone()))
                    .unwrap();
            }
            let weak = [first.downgrade(), second.downgrade()];
            queue
                .enqueue_traced(ReadJob {
                    capture: StoredObject::initial(Some(&runtime), &first).unwrap(),
                    nested: Some(StoredObject::initial(Some(&runtime), &second).unwrap()),
                    fail,
                })
                .unwrap();
            drop((first, second));
            assert_eq!(runtime.diagnostics().rooted, 1);
            runtime.verify_migration_barriers().unwrap();
            assert_eq!(runtime.collect_migration_graph().unwrap().retained, 3);
            assert_eq!(queue.run_one().is_err(), fail);
            assert!(queue.is_empty());
            assert_eq!(runtime.collect_migration_graph().unwrap().retained, 1);
            assert!(weak.iter().all(|object| object.upgrade().is_none()));
            drop(queue);
            assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
        }
    }

    #[test]
    fn unrooted_queues_release_pending_jobs_and_reject_foreign_edges() {
        let runtime = Runtime::new();
        let queue = JobQueue::new(&runtime);
        let foreign_runtime = Runtime::new();
        let foreign = Object::new_in(&foreign_runtime);
        assert!(
            queue
                .enqueue_traced(ReadJob {
                    capture: StoredObject::initial(Some(&foreign_runtime), &foreign).unwrap(),
                    nested: None,
                    fail: false,
                })
                .is_err()
        );
        assert!(queue.is_empty());
        let object = Object::new_in(&runtime);
        let weak = object.downgrade();
        queue
            .enqueue_traced(ReadJob {
                capture: StoredObject::initial(Some(&runtime), &object).unwrap(),
                nested: None,
                fail: false,
            })
            .unwrap();
        drop((object, queue));
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn public_queue_clones_share_one_root_and_outlive_their_realm() {
        let runtime = Runtime::new();
        let realm = Realm::ordinary_in(&runtime);
        let weak = realm.global_object().downgrade();
        let queue = realm.job_queue();
        let clone = realm.job_queue();
        assert!(Rc::ptr_eq(&queue.0.0, &clone.0.0));
        drop(realm);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 1);
        assert!(weak.upgrade().is_none());
        queue.enqueue(|| Ok(()));
        drop(queue);
        assert_eq!(clone.drain().unwrap(), 1);
        drop(clone);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
    }

    #[test]
    fn draining_and_hook_state_survive_reentry_errors_and_panics() {
        let runtime = Runtime::new();
        let queue = JobQueue::new(&runtime);
        let hook_queue = queue.clone();
        queue.set_unhandled_rejection_hook(move |_| {
            hook_queue.set_unhandled_rejection_hook(|_| {});
            hook_queue.enqueue(|| Ok(()));
        });
        queue.report_unhandled(RuntimeError::type_error("rejection"));
        assert_eq!(queue.drain().unwrap(), 1);
        queue.enqueue(|| Err(RuntimeError::type_error("job error")));
        assert!(queue.drain().is_err());
        queue.enqueue(|| panic!("host job panic"));
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| queue.drain())).is_err());
        queue.enqueue(|| Ok(()));
        assert_eq!(queue.drain().unwrap(), 1);
        assert!(queue.0.borrow().active.is_empty());
        drop(queue);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
    }
}
