//! A `#[repr(transparent)]` wrapper under its inner type's key, run through the inner type's
//! vtable. Its memory always holds a valid `Inner`, which is what `RegisteredTask::from_raw_parts`
//! asks for; the same size and alignment alone would not be enough. 0.1.4's `RegisteredTask::new`
//! accepted it unchecked. `new` now checks the type and refuses it; `RegisteredTask::from_raw_parts`
//! (`unsafe`) takes it. 0.3.1 has no `from_raw_parts`, so this file does not build there.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::Poll;

use waker_tables::{
    derive_unsafe_wakeable_vtable, RegisteredTask, UnsafeWakeablePtr, UnsafeWakeableTask, VTableKey,
    GLOBAL_VTABLE_REGISTRY,
};

struct Inner {
    polls: u32,
    drops: Arc<AtomicUsize>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

// SAFETY: the functions only touch `self`.
unsafe impl UnsafeWakeableTask for Inner {
    unsafe fn poll(&mut self) -> Poll<()> {
        self.polls += 1;
        if self.polls >= 2 { Poll::Ready(()) } else { Poll::Pending }
    }

    unsafe fn wake(&self) {}

    unsafe fn drop_in_place(&mut self) {}
}

derive_unsafe_wakeable_vtable!(Inner, INNER_VTABLE);

/// `#[repr(transparent)]`: its memory is always a valid `Inner`.
#[repr(transparent)]
struct Wrapped(Inner);

// SAFETY: each function forwards to Inner's.
unsafe impl UnsafeWakeableTask for Wrapped {
    unsafe fn poll(&mut self) -> Poll<()> {
        // SAFETY: forwarded under the caller's contract.
        unsafe { self.0.poll() }
    }

    unsafe fn wake(&self) {
        // SAFETY: as above.
        unsafe { self.0.wake() }
    }

    unsafe fn drop_in_place(&mut self) {
        // SAFETY: as above.
        unsafe { self.0.drop_in_place() }
    }
}

const INNER_KEY: VTableKey = VTableKey { segment: "same_layout_key", kind: "inner", version: 1 };

fn wrapped(drops: &Arc<AtomicUsize>) -> Box<Wrapped> {
    Box::new(Wrapped(Inner { polls: 0, drops: drops.clone() }))
}

#[test]
fn a_transparent_wrapper_runs_under_the_inner_types_key() {
    GLOBAL_VTABLE_REGISTRY.register(INNER_KEY, &INNER_VTABLE);
    let drops = Arc::new(AtomicUsize::new(0));

    let vtable = GLOBAL_VTABLE_REGISTRY.get(&INNER_KEY).unwrap();
    assert!(vtable.is_for::<Inner>() && !vtable.is_for::<Wrapped>());
    let data = Box::into_raw(wrapped(&drops));

    // SAFETY: `data` is a live Wrapped, which Inner's vtable may read as an Inner (repr(transparent));
    // it stays valid until freed below, after the task is gone, and is used only through the task.
    let mut task = unsafe { RegisteredTask::from_raw_parts(INNER_KEY, UnsafeWakeablePtr { data: data as *mut (), vtable }) };
    // SAFETY: Inner's functions assume nothing beyond exclusive access.
    unsafe {
        assert_eq!(task.poll(), Poll::Pending);
        assert_eq!(task.poll(), Poll::Ready(()));
        task.wake();
        task.drop_in_place();
    }

    let (_, ptr) = task.into_raw_parts();
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    // SAFETY: `ptr.data` is the Box<Wrapped> from above, freed once, as the type it was boxed as.
    let back = unsafe { Box::from_raw(ptr.data as *mut Wrapped) };
    assert_eq!(back.0.polls, 2);
    drop(back);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn new_refuses_the_wrapper_under_the_inner_types_key() {
    GLOBAL_VTABLE_REGISTRY.register(INNER_KEY, &INNER_VTABLE);
    let drops = Arc::new(AtomicUsize::new(0));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| RegisteredTask::new(wrapped(&drops), INNER_KEY)));
    assert!(result.is_err(), "the safe constructor keeps its type check");
    assert_eq!(drops.load(Ordering::SeqCst), 1, "the refused task is dropped, not leaked");
}
