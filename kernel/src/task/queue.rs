use alloc::{boxed::Box, collections::vec_deque::VecDeque};

use crate::{spin_sync::SpinMutex, task::Task};

// TODO create a lock-free structure for this
pub struct TaskQueue {
    inner: SpinMutex<VecDeque<Box<Task>>>,
}

unsafe impl Sync for TaskQueue {}

impl TaskQueue {
    #[inline]
    pub const fn new() -> Self {
        Self {
            inner: SpinMutex::new(VecDeque::new()),
        }
    }

    #[inline]
    pub fn pop_front(&self) -> Option<Box<Task>> {
        self.inner.lock().pop_front()
    }

    #[inline]
    pub fn push_back(&self, task: Box<Task>) {
        self.inner.lock().push_back(task)
    }
}

pub static TASK_QUEUE: TaskQueue = TaskQueue::new();
