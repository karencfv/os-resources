use super::{Task, TaskId};
use alloc::task::Wake;
use alloc::{collections::BTreeMap, sync::Arc};
use core::task::Waker;
use core::task::{Context, Poll};
use crossbeam_queue::ArrayQueue;
use x86_64::instructions::interrupts::{self, enable_and_hlt};

pub struct Executor {
    // Instead of storing tasks in a VecDeque like we did for our SimpleExecutor,
    // we use a task_queue of task IDs and a BTreeMap named tasks that contains
    // the actual Task instances. The map is indexed by the TaskId to allow efficient
    // continuation of a specific task.
    tasks: BTreeMap<TaskId, Task>,
    // use Arc<ArrayQueue> for the task_queue because it will be shared
    // between the executor and wakers. The idea is that the wakers push
    // the ID of the woken task to the queue.
    task_queue: Arc<ArrayQueue<TaskId>>,
    // This map caches the Waker of a task after its creation. This has two reasons:
    // First, it improves performance by reusing the same waker for multiple wake-ups
    // of the same task instead of creating a new waker each time.
    // Second, it ensures that reference-counted wakers are not deallocated inside
    // interrupt handlers because it could lead to deadlocks
    waker_cache: BTreeMap<TaskId, Waker>,
}

impl Executor {
    pub fn new() -> Self {
        Executor {
            tasks: BTreeMap::new(),
            // capacity of 100 for the task_queue, which should be enough for now
            // In case our system will have more than 100 concurrent tasks at some point,
            // we can easily increase this size.
            task_queue: Arc::new(ArrayQueue::new(100)),
            waker_cache: BTreeMap::new(),
        }
    }
    pub fn spawn(&mut self, task: Task) {
        let task_id = task.id;
        if self.tasks.insert(task.id, task).is_some() {
            // this should never happen
            panic!("task with the same ID already exists")
        }
        self.task_queue.push(task_id).expect("queue full");
    }
    fn run_ready_tasks(&mut self) {
        // destructure self to avoid borrow checker errors
        let Self {
            tasks,
            task_queue,
            waker_cache,
        } = self;

        // Loop over all tasks in the task_queue, create a waker for each task, and then poll it.
        while let Ok(task_id) = task_queue.pop() {
            // For each popped task ID, we retrieve a mutable reference to the corresponding task
            // from the tasks map. Since our ScancodeStream implementation registers wakers before
            // checking whether a task needs to be put to sleep, it might happen that a wake-up occurs
            // for a task that no longer exists. In this case, we simply ignore the wake-up and continue
            // with the next ID from the queue.
            let task = match tasks.get_mut(&task_id) {
                Some(task) => task,
                None => continue, // task no longer exists
            };
            // Allow TaskWaker implementation take care of of adding woken tasks back to the queue.
            let waker = waker_cache
                .entry(task_id)
                .or_insert_with(|| TaskWaker::new(task_id, task_queue.clone()));
            let mut context = Context::from_waker(waker);
            match task.poll(&mut context) {
                // A task is finished when it returns Poll::Ready. In that case, we remove it from
                // the tasks map using the BTreeMap::remove method. We also remove its cached waker, if it exists.
                Poll::Ready(()) => {
                    // task done -> remove it and its cached waker
                    tasks.remove(&task_id);
                    waker_cache.remove(&task_id);
                }
                Poll::Pending => {}
            }
        }
    }
    pub fn run(&mut self) -> ! {
        // While we could theoretically return from the function when the tasks map
        // becomes empty, this would never happen since our keyboard_task never finishes,
        // so a simple loop should suffice
        loop {
            self.run_ready_tasks();
            self.sleep_if_idle();
        }
    }
    pub fn sleep_if_idle(&self) {
        if self.task_queue.is_empty() {
            // HLT (halt) is an assembly language instruction which halts the
            // central processing unit (CPU) until the next external interrupt is fired.

            // disable interrupts on the CPU before the check and atomically enable them
            // again together with the hlt instruction. This way, all interrupts that happen
            // in between are delayed after the hlt instruction so that no wake-ups are missed.
            interrupts::disable();
            if self.task_queue.is_empty() {
                enable_and_hlt();
            } else {
                interrupts::enable();
            }
        }
    }
}

struct TaskWaker {
    task_id: TaskId,
    task_queue: Arc<ArrayQueue<TaskId>>,
}

impl TaskWaker {
    fn new(task_id: TaskId, task_queue: Arc<ArrayQueue<TaskId>>) -> Waker {
        // wrap the TaskWaker in an Arc and use the Waker::from implementation to convert it to a Waker.
        // This from method takes care of constructing a RawWakerVTable and a RawWaker instance
        // for our TaskWaker type.
        Waker::from(Arc::new(TaskWaker {
            task_id,
            task_queue,
        }))
    }

    // We push the task_id to the referenced task_queue. Since modifications
    // of the ArrayQueue type only require a shared reference, we can implement
    // this method on &self instead of &mut self
    fn wake_task(&self) {
        self.task_queue.push(self.task_id).expect("task queue full");
    }
}

// In order to use our TaskWaker type for polling futures, we need to convert it to
// a Waker instance first. This is required because the Future::poll method takes a
// Context instance as argument, which can only be constructed from the Waker type.
impl Wake for TaskWaker {
    // Since wakers are commonly shared between the executor and the asynchronous tasks,
    // the trait methods require that the Self instance is wrapped in the Arc type,
    // which implements reference-counted ownership.

    fn wake(self: Arc<Self>) {
        self.wake_task();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.wake_task();
    }
}
