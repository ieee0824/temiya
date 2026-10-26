//! FIFO Promise-job queue owned by a Realm.

use std::{cell::RefCell, collections::VecDeque, fmt, rc::Rc};

use super::{RuntimeError, RuntimeResult};

type Job = Box<dyn FnOnce() -> RuntimeResult<()>>;
type UnhandledHook = Rc<dyn Fn(RuntimeError)>;

#[derive(Default)]
struct JobQueueData {
    jobs: VecDeque<Job>,
    draining: bool,
    unhandled_hook: Option<UnhandledHook>,
}

/// A Realm-local FIFO queue for Promise reactions and thenable jobs.
#[derive(Clone, Default)]
pub struct JobQueue(Rc<RefCell<JobQueueData>>);

impl fmt::Debug for JobQueue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JobQueue")
            .field("len", &self.len())
            .finish_non_exhaustive()
    }
}

impl JobQueue {
    /// Creates an empty queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueues one job at the tail of the queue.
    pub fn enqueue(&self, job: impl FnOnce() -> RuntimeResult<()> + 'static) {
        self.0.borrow_mut().jobs.push_back(Box::new(job));
    }

    /// Returns the number of jobs waiting to run.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.borrow().jobs.len()
    }

    /// Returns whether the queue has no pending jobs.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Runs one queued job, returning whether a job was available.
    pub fn run_one(&self) -> RuntimeResult<bool> {
        let job = self.0.borrow_mut().jobs.pop_front();
        let Some(job) = job else { return Ok(false) };
        job()?;
        Ok(true)
    }

    /// Drains the queue to a microtask checkpoint, including jobs enqueued by jobs.
    pub fn drain(&self) -> RuntimeResult<usize> {
        if self.0.borrow().draining {
            return Err(RuntimeError::type_error("Job Queue is already draining"));
        }
        self.0.borrow_mut().draining = true;
        let result = (|| {
            let mut count = 0;
            while self.run_one()? {
                count += 1;
            }
            Ok(count)
        })();
        self.0.borrow_mut().draining = false;
        result
    }

    /// Installs the host callback used for still-unhandled rejections.
    pub fn set_unhandled_rejection_hook(&self, hook: impl Fn(RuntimeError) + 'static) {
        self.0.borrow_mut().unhandled_hook = Some(Rc::new(hook));
    }

    pub(super) fn report_unhandled(&self, error: RuntimeError) {
        if let Some(hook) = self.0.borrow().unhandled_hook.clone() {
            hook(error);
        }
    }
}
