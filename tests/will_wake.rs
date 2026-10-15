//! `will_wake` between a waker and its clones. `Waker::will_wake` compares vtable addresses, so it
//! holds only if every waker of a type carries one `&'static RawWakerVTable`. Its own file, because
//! whether two copies of a vtable appear depends on how the crate is split for codegen. On 0.3.0
//! the clone check below failed under `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=256 cargo test --release`.

use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::Context;

use waker_tables::{derive_arc_unsafe_wakeable, waker_from_arc, UnsafeWakeable};

struct Signal {
    wakes: AtomicUsize,
}

impl Signal {
    fn on_wake(self: Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
    }
}

derive_arc_unsafe_wakeable!(Signal, on_wake);

/// A waker, its clones (by `Waker::clone` and through a `Context`) and a second waker of the same
/// `Arc` all hold one vtable address, so `will_wake` holds between them in every build profile.
#[test]
fn a_waker_and_its_clones_share_one_vtable() {
    let signal = Arc::new(Signal { wakes: AtomicUsize::new(0) });
    let waker = waker_from_arc(signal.clone());
    let declared = black_box(<Signal as UnsafeWakeable>::raw_waker_vtable() as *const _);
    assert_eq!(black_box(waker.vtable() as *const _), declared, "the waker does not carry Signal's static");
    let wakers = [
        waker.clone(),
        Context::from_waker(&waker).waker().clone(),
        waker_from_arc(signal.clone()),
    ];
    for other in &wakers {
        let (a, b) = (black_box(waker.vtable() as *const _), black_box(other.vtable() as *const _));
        assert_eq!(a, b, "two addresses for Signal's vtable");
        assert!(other.will_wake(&waker));
    }
    drop((waker, wakers));
    assert_eq!(Arc::strong_count(&signal), 1);
    assert_eq!(signal.wakes.load(Ordering::SeqCst), 0);
}

/// A hand-written impl that keeps the default `raw_waker_vtable`.
struct Manual {
    wakes: AtomicUsize,
}

// SAFETY: every fn moves exactly the one strong ref its contract names; `raw_waker_vtable` keeps
// its default, `Manual`'s own vtable.
unsafe impl UnsafeWakeable for Manual {
    unsafe fn wake(ptr: *const Self) {
        // SAFETY: `ptr` owns one strong ref (trait contract); it is consumed here.
        let arc = unsafe { Arc::from_raw(ptr) };
        arc.wakes.fetch_add(1, Ordering::SeqCst);
    }

    unsafe fn wake_by_ref(ptr: *const Self) {
        // SAFETY: `ptr` points at a live Arc whose ref the caller keeps.
        unsafe { &*ptr }.wakes.fetch_add(1, Ordering::SeqCst);
    }

    unsafe fn clone_ptr(ptr: *const Self) -> *const Self {
        // SAFETY: `ptr` points at a live Arc; the count goes up by exactly one.
        unsafe { Arc::increment_strong_count(ptr) };
        ptr
    }

    unsafe fn drop_ptr(ptr: *const Self) {
        // SAFETY: `ptr` owns one strong ref (trait contract); it is released here.
        unsafe { Arc::decrement_strong_count(ptr) };
    }
}

/// Wakers of a hand-written impl wake and count refs right. `will_wake` between them is
/// best-effort (the default vtable is a constant), so it is not asserted.
#[test]
fn a_hand_written_impl_keeps_the_default_vtable() {
    let manual = Arc::new(Manual { wakes: AtomicUsize::new(0) });
    let waker = waker_from_arc(manual.clone());
    let clone = Context::from_waker(&waker).waker().clone();
    assert_eq!(Arc::strong_count(&manual), 3);

    clone.wake();
    waker.wake_by_ref();
    assert_eq!(manual.wakes.load(Ordering::SeqCst), 2);
    assert_eq!(Arc::strong_count(&manual), 2);

    drop(waker);
    assert_eq!(Arc::strong_count(&manual), 1);
}
