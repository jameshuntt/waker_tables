//! Hand-built task vtables and reading their functions: `UnsafeWakeableVTable::from_raw_parts` (in a
//! `static`) and `poll_fn()`, `wake_fn()`, `drop_fn()`. 0.1.4 wrote these as a struct literal and
//! read its `pub` fields. 0.3.1 has neither, so this file does not build there.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::Poll;

use waker_tables::{
    derive_unsafe_wakeable_vtable, RegisteredTask, UnsafeWakeablePtr, UnsafeWakeableTask, UnsafeWakeableVTable,
    VTableKey, GLOBAL_VTABLE_REGISTRY,
};

/// Counts its polls, wakes, cleanups and drops.
struct Counter {
    polls: u32,
    log: Arc<Log>,
}

#[derive(Default)]
struct Log {
    wakes: AtomicUsize,
    cleanups: AtomicUsize,
    drops: AtomicUsize,
}

impl Log {
    fn get(counter: &AtomicUsize) -> usize {
        counter.load(Ordering::SeqCst)
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        self.log.drops.fetch_add(1, Ordering::SeqCst);
    }
}

// SAFETY: the functions only touch `self` and its log.
unsafe impl UnsafeWakeableTask for Counter {
    unsafe fn poll(&mut self) -> Poll<()> {
        self.polls += 1;
        if self.polls >= 2 { Poll::Ready(()) } else { Poll::Pending }
    }

    unsafe fn wake(&self) {
        self.log.wakes.fetch_add(1, Ordering::SeqCst);
    }

    unsafe fn drop_in_place(&mut self) {
        self.log.cleanups.fetch_add(1, Ordering::SeqCst);
    }
}

derive_unsafe_wakeable_vtable!(Counter, COUNTER_VTABLE);

// Hand-built functions that differ from the trait's, so a test can tell which ran: polls go by ten.
unsafe fn poll_by_ten(data: *mut ()) -> Poll<()> {
    // SAFETY: from_raw_parts' contract: `data` is a live Counter that nothing else uses meanwhile.
    let counter = unsafe { &mut *(data as *mut Counter) };
    counter.polls += 10;
    if counter.polls >= 20 { Poll::Ready(()) } else { Poll::Pending }
}

unsafe fn wake_by_ten(data: *const ()) {
    // SAFETY: from_raw_parts' contract: `data` is a live Counter.
    let counter = unsafe { &*(data as *const Counter) };
    counter.log.wakes.fetch_add(10, Ordering::SeqCst);
}

unsafe fn cleanup_by_ten(data: *mut ()) {
    // SAFETY: as `poll_by_ten`.
    let counter = unsafe { &mut *(data as *mut Counter) };
    counter.log.cleanups.fetch_add(10, Ordering::SeqCst);
}

// SAFETY: each function reads its pointer as a live Counter with the access from_raw_parts names.
static BY_TEN: UnsafeWakeableVTable =
    unsafe { UnsafeWakeableVTable::from_raw_parts::<Counter>(poll_by_ten, wake_by_ten, cleanup_by_ten) };

// 0.1.4's macro built its vtable from closures; they coerce the same way. The closures sit inside
// the `unsafe` block, which covers their bodies.
// SAFETY: each closure reads its pointer as a live Counter with the access from_raw_parts names and
// calls Counter's trait function.
static BY_CLOSURES: UnsafeWakeableVTable = unsafe {
    UnsafeWakeableVTable::from_raw_parts::<Counter>(
        |ptr| (*(ptr as *mut Counter)).poll(),
        |ptr| (*(ptr as *const Counter)).wake(),
        |ptr| (*(ptr as *mut Counter)).drop_in_place(),
    )
};

/// A second task type, for the type check.
struct Other;

// SAFETY: the functions touch nothing.
unsafe impl UnsafeWakeableTask for Other {
    unsafe fn poll(&mut self) -> Poll<()> {
        Poll::Ready(())
    }

    unsafe fn wake(&self) {}

    unsafe fn drop_in_place(&mut self) {}
}

fn key(kind: &'static str) -> VTableKey {
    VTableKey { segment: "task_vtable_parts", kind, version: 1 }
}

#[test]
fn a_hand_built_vtable_records_its_type() {
    for vtable in [&BY_TEN, &BY_CLOSURES] {
        assert!(vtable.is_for::<Counter>());
        assert!(!vtable.is_for::<Other>());
        assert_eq!(vtable.type_name(), "task_vtable_parts::Counter");
    }
}

#[test]
fn registered_task_new_accepts_it_and_runs_the_hand_built_functions() {
    let k = key("by-ten");
    GLOBAL_VTABLE_REGISTRY.register(k, &BY_TEN);
    let log = Arc::new(Log::default());

    let mut task = RegisteredTask::new(Box::new(Counter { polls: 0, log: log.clone() }), k);
    // SAFETY: the hand-built functions assume nothing beyond exclusive access.
    unsafe {
        assert_eq!(task.poll(), Poll::Pending, "10 polls");
        assert_eq!(task.poll(), Poll::Ready(()), "20 polls");
        task.wake();
        task.drop_in_place();
    }
    assert_eq!((Log::get(&log.wakes), Log::get(&log.cleanups)), (10, 10));
    drop(task);
    assert_eq!(Log::get(&log.drops), 1);
}

#[test]
fn a_closure_built_vtable_runs_the_trait_functions() {
    let k = key("closures");
    GLOBAL_VTABLE_REGISTRY.register(k, &BY_CLOSURES);
    let log = Arc::new(Log::default());

    let mut task = RegisteredTask::new(Box::new(Counter { polls: 0, log: log.clone() }), k);
    // SAFETY: Counter's functions assume nothing beyond exclusive access.
    unsafe {
        assert_eq!(task.poll(), Poll::Pending);
        assert_eq!(task.poll(), Poll::Ready(()));
        task.wake();
        task.drop_in_place();
    }
    assert_eq!((Log::get(&log.wakes), Log::get(&log.cleanups)), (1, 1));
    drop(task);
    assert_eq!(Log::get(&log.drops), 1);
}

#[test]
fn the_functions_can_be_read_and_called() {
    let log = Arc::new(Log::default());
    let mut counter = Counter { polls: 0, log: log.clone() };
    let data = &mut counter as *mut Counter as *mut ();

    let (poll, wake, cleanup) = (BY_TEN.poll_fn(), BY_TEN.wake_fn(), BY_TEN.drop_fn());
    // SAFETY: `data` is a live Counter used only through these calls, one at a time.
    unsafe {
        assert_eq!(poll(data), Poll::Pending);
        wake(data as *const ());
        cleanup(data);
    }
    assert_eq!(counter.polls, 10);

    let (poll, wake, cleanup) = (COUNTER_VTABLE.poll_fn(), COUNTER_VTABLE.wake_fn(), COUNTER_VTABLE.drop_fn());
    let data = &mut counter as *mut Counter as *mut ();
    // SAFETY: as above; the derived vtable's functions run Counter's trait functions.
    unsafe {
        assert_eq!(poll(data), Poll::Ready(()));
        wake(data as *const ());
        cleanup(data);
    }
    assert_eq!(counter.polls, 11);
    assert_eq!((Log::get(&log.wakes), Log::get(&log.cleanups)), (11, 11));
    drop(counter);
    assert_eq!(Log::get(&log.drops), 1);
}

#[test]
fn a_ptr_reads_its_functions_from_a_hand_built_vtable() {
    let log = Arc::new(Log::default());
    let data = Box::into_raw(Box::new(Counter { polls: 0, log: log.clone() }));
    let mut ptr = UnsafeWakeablePtr { data: data as *mut (), vtable: &BY_TEN };
    // SAFETY: `ptr` holds a live Counter with a vtable built for Counter, used only here.
    unsafe {
        assert_eq!(ptr.poll(), Poll::Pending);
        assert_eq!(ptr.poll(), Poll::Ready(()));
    }
    // SAFETY: `data` came from Box::into_raw above and is freed once.
    drop(unsafe { Box::from_raw(data) });
    assert_eq!(Log::get(&log.drops), 1);
}

#[test]
fn new_still_refuses_a_hand_built_vtable_for_another_type() {
    let k = key("by-ten-other");
    GLOBAL_VTABLE_REGISTRY.register(k, &BY_TEN);
    let result = std::panic::catch_unwind(|| RegisteredTask::new(Box::new(Other), k));
    assert!(result.is_err());
}
