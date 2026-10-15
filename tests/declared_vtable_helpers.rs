//! The four helper fns `declare_raw_waker_vtable!` leaves at the call site, with 0.1.4's names and
//! signatures: `__waker_clone`, `__waker_wake`, `__waker_wake_by_ref`, `__waker_drop`. 0.3.1's macro
//! leaves none, so this file does not build there.

use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{RawWaker, Waker};

use waker_tables::{declare_raw_waker_vtable, derive_arc_unsafe_wakeable};

struct Signal {
    wakes: AtomicUsize,
}

impl Signal {
    fn on_wake(self: Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
    }
}

derive_arc_unsafe_wakeable!(Signal, on_wake);
declare_raw_waker_vtable!(Signal, SIGNAL_DECLARED);

#[test]
fn the_helpers_are_at_the_call_site_with_their_signatures() {
    let clone: unsafe fn(*const ()) -> RawWaker = __waker_clone;
    let wake: unsafe fn(*const ()) = __waker_wake;
    let wake_by_ref: unsafe fn(*const ()) = __waker_wake_by_ref;
    let drop_ref: unsafe fn(*const ()) = __waker_drop;

    let signal = Arc::new(Signal { wakes: AtomicUsize::new(0) });
    let data = Arc::into_raw(signal.clone()) as *const ();
    assert_eq!(Arc::strong_count(&signal), 2);

    // SAFETY: `data` is the `*const Signal` of a live Arc and owns one strong ref; the clone's
    // RawWaker owns the new ref and carries a vtable declared for Signal. The Waker is only read
    // (ManuallyDrop), and its ref is given up by `wake` below.
    let cloned = ManuallyDrop::new(unsafe { Waker::from_raw(clone(data)) });
    assert!(std::ptr::eq(cloned.vtable(), &SIGNAL_DECLARED), "the clone points at the declared static");
    assert_eq!(cloned.data(), data);
    assert_eq!(Arc::strong_count(&signal), 3);

    // SAFETY: `data` points at a live Arc<Signal>; the ref is kept.
    unsafe { wake_by_ref(data) };
    assert_eq!(signal.wakes.load(Ordering::SeqCst), 1);
    assert_eq!(Arc::strong_count(&signal), 3);

    // SAFETY: `cloned.data()` owns the ref `clone` took; `wake` gives it up.
    unsafe { wake(cloned.data()) };
    assert_eq!(signal.wakes.load(Ordering::SeqCst), 2);
    assert_eq!(Arc::strong_count(&signal), 2);

    // SAFETY: `data` owns the ref `Arc::into_raw` gave it; it is given up here.
    unsafe { drop_ref(data) };
    assert_eq!(Arc::strong_count(&signal), 1);
}
