//! `RegisteredTask::owns_its_task`: whether the pointer `into_raw_parts` hands back is a `Box<T>`
//! the caller must free (a task made by `new`) or memory the caller already owns (a task made by
//! `from_raw_parts`). `into_raw_parts` returns only the key and the pointer, so code that takes a
//! `RegisteredTask` it did not build reads this first. e5833df has no `owns_its_task`, so this
//! file does not build there.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::Poll;

use waker_tables::{
    derive_unsafe_wakeable_vtable, RegisteredTask, UnsafeWakeablePtr, UnsafeWakeableTask, VTableKey,
    GLOBAL_VTABLE_REGISTRY,
};

/// Ready after `ready_after` polls; counts its drops.
struct Job {
    ready_after: u32,
    polls: u32,
    drops: Arc<AtomicUsize>,
}

impl Job {
    fn new(ready_after: u32, drops: &Arc<AtomicUsize>) -> Self {
        Self { ready_after, polls: 0, drops: drops.clone() }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

// SAFETY: the functions only touch `self`.
unsafe impl UnsafeWakeableTask for Job {
    unsafe fn poll(&mut self) -> Poll<()> {
        self.polls += 1;
        if self.polls >= self.ready_after { Poll::Ready(()) } else { Poll::Pending }
    }

    unsafe fn wake(&self) {}

    unsafe fn drop_in_place(&mut self) {}
}

derive_unsafe_wakeable_vtable!(Job, JOB_VTABLE);

const KEY: VTableKey = VTableKey { segment: "owns_its_task", kind: "job", version: 1 };

fn count(drops: &AtomicUsize) -> usize {
    drops.load(Ordering::SeqCst)
}

#[test]
fn new_owns_its_task_and_from_raw_parts_does_not() {
    GLOBAL_VTABLE_REGISTRY.register(KEY, &JOB_VTABLE);
    let drops = Arc::new(AtomicUsize::new(0));

    let boxed = RegisteredTask::new(Box::new(Job::new(1, &drops)), KEY);
    assert!(boxed.owns_its_task(), "`new` boxed the task and frees it");

    let mut job = Job::new(1, &drops);
    let ptr = UnsafeWakeablePtr { data: &mut job as *mut Job as *mut (), vtable: &JOB_VTABLE };
    // SAFETY: `job` is a live Job, outlives `lent`, and is used only through `lent` meanwhile.
    let lent = unsafe { RegisteredTask::from_raw_parts(KEY, ptr) };
    assert!(!lent.owns_its_task(), "the caller owns the memory under `from_raw_parts`");

    drop(boxed);
    drop(lent);
    assert_eq!(count(&drops), 1, "only the boxed task was freed");
    drop(job);
    assert_eq!(count(&drops), 2);
}

/// The scheduler: it knows the task type (it frees a `Box<Job>`) but not who made each task.
/// It runs each task to the end, takes its pointer back, and frees only the ones it owns.
fn run_and_free(queue: Vec<RegisteredTask>) -> usize {
    let mut freed = 0;
    for mut task in queue {
        // SAFETY: Job's functions assume nothing beyond exclusive access.
        while unsafe { task.poll() }.is_pending() {}
        let owned = task.owns_its_task();
        let (_, ptr) = task.into_raw_parts();
        if owned {
            // SAFETY: owns_its_task was true, so `new` boxed this Job and the scheduler is now its
            // only owner; it is freed once, here.
            drop(unsafe { Box::from_raw(ptr.data as *mut Job) });
            freed += 1;
        }
    }
    freed
}

/// Two producers fill one queue: one boxes its jobs with `new`, the other lends jobs it keeps on
/// its stack through `from_raw_parts`. Under Miri a boxed job left unfreed is a leak, and a lent
/// one freed as a `Box` is Undefined Behavior, so this passes only if `owns_its_task` is right.
#[test]
fn a_scheduler_frees_only_the_tasks_it_owns() {
    GLOBAL_VTABLE_REGISTRY.register(KEY, &JOB_VTABLE);
    let boxed_drops = Arc::new(AtomicUsize::new(0));
    let lent_drops = Arc::new(AtomicUsize::new(0));
    let mut lent = [Job::new(2, &lent_drops), Job::new(3, &lent_drops)];

    let mut queue = vec![RegisteredTask::new(Box::new(Job::new(1, &boxed_drops)), KEY)];
    for job in lent.iter_mut() {
        let ptr = UnsafeWakeablePtr { data: job as *mut Job as *mut (), vtable: &JOB_VTABLE };
        // SAFETY: each lent job is a live Job that outlives the queue and is used only through its
        // task until `run_and_free` returns.
        queue.push(unsafe { RegisteredTask::from_raw_parts(KEY, ptr) });
    }
    queue.push(RegisteredTask::new(Box::new(Job::new(2, &boxed_drops)), KEY));

    assert_eq!(run_and_free(queue), 2, "the two boxed jobs");
    assert_eq!(count(&boxed_drops), 2);
    assert_eq!(count(&lent_drops), 0, "the lent jobs are still the producer's");
    assert_eq!((lent[0].polls, lent[1].polls), (2, 3));
    drop(lent);
    assert_eq!(count(&lent_drops), 2);
}
