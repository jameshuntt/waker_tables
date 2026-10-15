//! A `RawWakerVTable` chosen per waker: `waker_from_arc_with_vtable` (`unsafe`) and
//! `raw_waker_from_ptr_with_vtable` (safe), 0.1.4's `waker_from_arc(arc, &VTABLE)` and
//! `raw_waker_from_ptr(ptr, &VTABLE)`. 0.3.1 has neither, so this file does not build there.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, RawWaker, RawWakerVTable, Waker};

use waker_tables::{
    derive_arc_unsafe_wakeable, raw_waker_from_ptr_with_vtable, waker_from_arc, waker_from_arc_with_vtable,
    UnsafeWakeable,
};

struct Signal {
    wakes: AtomicUsize,
    loud: AtomicUsize,
}

impl Signal {
    fn new() -> Arc<Self> {
        Arc::new(Self { wakes: AtomicUsize::new(0), loud: AtomicUsize::new(0) })
    }

    fn on_wake(self: Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
    }

    fn wakes(&self) -> usize {
        self.wakes.load(Ordering::SeqCst)
    }

    fn loud(&self) -> usize {
        self.loud.load(Ordering::SeqCst)
    }
}

derive_arc_unsafe_wakeable!(Signal, on_wake);

/// A second vtable for `Signal`: every wake also counts in `loud`, then runs Signal's own wake.
/// Refs move by Signal's `UnsafeWakeable` functions, and its clone returns a RawWaker on `LOUD`.
static LOUD: RawWakerVTable = RawWakerVTable::new(loud_clone, loud_wake, loud_wake_by_ref, loud_drop);

unsafe fn loud_clone(data: *const ()) -> RawWaker {
    // SAFETY: a waker on LOUD carries the `*const Signal` of a live Arc; the new ref belongs to the
    // returned RawWaker.
    let ptr = unsafe { Signal::clone_ptr(data as *const Signal) };
    RawWaker::new(ptr as *const (), &LOUD)
}

unsafe fn loud_wake(data: *const ()) {
    let ptr = data as *const Signal;
    // SAFETY: `ptr` owns one strong ref of a live Arc<Signal>; `wake` gives it up, after the count.
    unsafe {
        (*ptr).loud.fetch_add(1, Ordering::SeqCst);
        Signal::wake(ptr);
    }
}

unsafe fn loud_wake_by_ref(data: *const ()) {
    let ptr = data as *const Signal;
    // SAFETY: `ptr` points at a live Arc<Signal> whose ref the waker keeps.
    unsafe {
        (*ptr).loud.fetch_add(1, Ordering::SeqCst);
        Signal::wake_by_ref(ptr);
    }
}

unsafe fn loud_drop(data: *const ()) {
    // SAFETY: `data` owns one strong ref of an Arc<Signal>; it is given up here.
    unsafe { Signal::drop_ptr(data as *const Signal) }
}

fn is_loud(waker: &Waker) -> bool {
    std::ptr::eq(waker.vtable(), &LOUD)
}

#[test]
fn a_waker_runs_the_vtable_it_was_given_and_keeps_it_through_clones() {
    let signal = Signal::new();
    // SAFETY: LOUD reads its data as an Arc<Signal> under Signal's refcount rules and clones onto LOUD.
    let waker = unsafe { waker_from_arc_with_vtable(signal.clone(), &LOUD) };
    assert!(is_loud(&waker));

    waker.wake_by_ref();
    assert_eq!((signal.wakes(), signal.loud()), (1, 1));
    assert_eq!(Arc::strong_count(&signal), 2);

    let clones = [waker.clone(), Context::from_waker(&waker).waker().clone()];
    assert_eq!(Arc::strong_count(&signal), 4);
    for clone in &clones {
        assert!(is_loud(clone), "a clone left LOUD");
        assert!(clone.will_wake(&waker));
    }

    let [a, b] = clones;
    a.wake();
    drop(b);
    assert_eq!((signal.wakes(), signal.loud()), (2, 2));
    assert_eq!(Arc::strong_count(&signal), 2);

    waker.wake();
    assert_eq!((signal.wakes(), signal.loud()), (3, 3));
    assert_eq!(Arc::strong_count(&signal), 1);
}

#[test]
fn two_wakers_of_one_type_carry_different_vtables() {
    let signal = Signal::new();
    let plain = waker_from_arc(signal.clone());
    // SAFETY: as above.
    let loud = unsafe { waker_from_arc_with_vtable(signal.clone(), &LOUD) };
    assert!(!is_loud(&plain));
    assert!(!plain.will_wake(&loud));

    plain.wake();
    assert_eq!((signal.wakes(), signal.loud()), (1, 0), "waker_from_arc is unchanged");
    loud.wake();
    assert_eq!((signal.wakes(), signal.loud()), (2, 1));
    assert_eq!(Arc::strong_count(&signal), 1);
}

#[test]
fn raw_waker_from_ptr_with_vtable_builds_the_same_waker() {
    let signal = Signal::new();
    // Safe, as 0.1.4's raw_waker_from_ptr(ptr, &VTABLE) was: the RawWaker does nothing until
    // Waker::from_raw below.
    let raw = raw_waker_from_ptr_with_vtable(Arc::into_raw(signal.clone()), &LOUD);
    // SAFETY: `raw` owns a strong ref of an Arc<Signal> and carries LOUD, which reads its data as an
    // Arc<Signal> under Signal's refcount rules and clones onto LOUD.
    let waker = unsafe { Waker::from_raw(raw) };
    assert!(is_loud(&waker));
    assert_eq!(waker.data(), Arc::as_ptr(&signal) as *const ());
    let clone = waker.clone();
    assert!(is_loud(&clone));
    clone.wake();
    waker.wake_by_ref();
    assert_eq!((signal.wakes(), signal.loud()), (2, 2));
    drop(waker);
    assert_eq!(Arc::strong_count(&signal), 1);
}

#[test]
fn the_types_own_vtable_passed_explicitly_matches_waker_from_arc() {
    let signal = Signal::new();
    // SAFETY: Signal's own vtable meets the contract.
    let explicit = unsafe { waker_from_arc_with_vtable(signal.clone(), Signal::raw_waker_vtable()) };
    let implicit = waker_from_arc(signal.clone());
    assert!(explicit.will_wake(&implicit));
    explicit.wake();
    implicit.wake();
    assert_eq!((signal.wakes(), signal.loud()), (2, 0));
    assert_eq!(Arc::strong_count(&signal), 1);
}
