//! `RegisteredTask` over memory the caller owns, and taking its pointer back out:
//! `RegisteredTask::from_raw_parts` and `into_raw_parts`. 0.1.4 wrote these as the struct literal
//! `RegisteredTask { key, ptr }` and moved `ptr` out of it, and never freed a task. 0.3.1 has
//! neither, so this file does not build there.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::Poll;

use waker_tables::{
    derive_unsafe_wakeable_vtable, RegisteredTask, UnsafeWakeablePtr, UnsafeWakeableTask, VTableKey,
    GLOBAL_VTABLE_REGISTRY,
};

/// Ready after `ready_after` polls; counts its drops.
struct Countdown {
    ready_after: u32,
    polls: u32,
    drops: Arc<AtomicUsize>,
}

impl Drop for Countdown {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

// SAFETY: the functions only touch `self`.
unsafe impl UnsafeWakeableTask for Countdown {
    unsafe fn poll(&mut self) -> Poll<()> {
        self.polls += 1;
        if self.polls >= self.ready_after { Poll::Ready(()) } else { Poll::Pending }
    }

    unsafe fn wake(&self) {}

    unsafe fn drop_in_place(&mut self) {}
}

derive_unsafe_wakeable_vtable!(Countdown, COUNTDOWN_VTABLE);

fn key(kind: &'static str) -> VTableKey {
    VTableKey { segment: "registered_task_parts", kind, version: 1 }
}

fn drops(counter: &AtomicUsize) -> usize {
    counter.load(Ordering::SeqCst)
}

#[test]
fn a_task_on_the_stack_is_run_and_never_freed_by_registered_task() {
    let counted = Arc::new(AtomicUsize::new(0));
    let mut countdown = Countdown { ready_after: 3, polls: 0, drops: counted.clone() };
    let k = key("stack");
    let ptr = UnsafeWakeablePtr { data: &mut countdown as *mut Countdown as *mut (), vtable: &COUNTDOWN_VTABLE };

    // SAFETY: `countdown` is a live Countdown, outlives `task`, and is used only through `task`
    // while `task` exists.
    let mut task = unsafe { RegisteredTask::from_raw_parts(k, ptr) };
    assert_eq!(task.key(), k, "the key is kept as given; it was never registered");
    // SAFETY: Countdown's functions assume nothing beyond exclusive access.
    while unsafe { task.poll() }.is_pending() {}
    drop(task);

    assert_eq!(drops(&counted), 0, "RegisteredTask's Drop left the caller's memory alone");
    assert_eq!(countdown.polls, 3);
    drop(countdown);
    assert_eq!(drops(&counted), 1);
}

#[test]
fn into_raw_parts_hands_a_new_task_back_without_freeing_it() {
    let k = key("boxed");
    GLOBAL_VTABLE_REGISTRY.register(k, &COUNTDOWN_VTABLE);
    let counted = Arc::new(AtomicUsize::new(0));

    let task = RegisteredTask::new(Box::new(Countdown { ready_after: 2, polls: 0, drops: counted.clone() }), k);
    let data = task.ptr().data;
    let (back_key, mut ptr) = task.into_raw_parts();
    assert_eq!(back_key, k);
    assert_eq!(ptr.data, data);
    assert!(ptr.vtable.is_for::<Countdown>());
    assert_eq!(drops(&counted), 0, "into_raw_parts does not free");

    // SAFETY: `ptr` holds the live Countdown `new` boxed, with its own vtable, used only here.
    unsafe {
        assert_eq!(ptr.poll(), Poll::Pending);
        assert_eq!(ptr.poll(), Poll::Ready(()));
    }
    // SAFETY: the caller now owns the Box<Countdown> `new` took; it is freed once, here.
    drop(unsafe { Box::from_raw(ptr.data as *mut Countdown) });
    assert_eq!(drops(&counted), 1);
}

#[test]
fn from_raw_parts_and_into_raw_parts_round_trip() {
    let counted = Arc::new(AtomicUsize::new(0));
    let data = Box::into_raw(Box::new(Countdown { ready_after: 2, polls: 0, drops: counted.clone() }));
    let k = key("round-trip");

    // SAFETY: `data` is a live, caller-owned Countdown, freed only after the task is gone.
    let mut task =
        unsafe { RegisteredTask::from_raw_parts(k, UnsafeWakeablePtr { data: data as *mut (), vtable: &COUNTDOWN_VTABLE }) };
    // SAFETY: Countdown's functions assume nothing beyond exclusive access.
    assert_eq!(unsafe { task.poll() }, Poll::Pending);

    let (k2, ptr) = task.into_raw_parts();
    assert_eq!(k2, k);
    assert_eq!(ptr.data, data as *mut ());

    // SAFETY: the same pointer and vtable, still live and caller-owned.
    let mut again = unsafe { RegisteredTask::from_raw_parts(k2, ptr) };
    // SAFETY: as above.
    assert_eq!(unsafe { again.poll() }, Poll::Ready(()));
    drop(again);
    assert_eq!(drops(&counted), 0);

    // SAFETY: `data` came from Box::into_raw above and is freed once.
    drop(unsafe { Box::from_raw(data) });
    assert_eq!(drops(&counted), 1);
}

#[test]
fn new_still_frees_its_task_on_drop() {
    let k = key("new-frees");
    GLOBAL_VTABLE_REGISTRY.register(k, &COUNTDOWN_VTABLE);
    let counted = Arc::new(AtomicUsize::new(0));
    drop(RegisteredTask::new(Box::new(Countdown { ready_after: 1, polls: 0, drops: counted.clone() }), k));
    assert_eq!(drops(&counted), 1);
}
