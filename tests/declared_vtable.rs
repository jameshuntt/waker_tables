//! `declare_raw_waker_vtable!` as in 0.1.4: the declared static's clone returns a `RawWaker` on that
//! same static, so a `Waker` built on it keeps it through `Waker::clone` and `Context` clones, and
//! `will_wake` holds between them. The waker is built with std alone, so this file builds on 0.3.1
//! too, and fails there: 0.3.1's static cloned onto the type's own vtable.

use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, RawWaker, RawWakerVTable, Waker};

use waker_tables::{declare_raw_waker_vtable, derive_arc_unsafe_wakeable};

struct Signal {
    wakes: AtomicUsize,
}

impl Signal {
    fn new() -> Arc<Self> {
        Arc::new(Self { wakes: AtomicUsize::new(0) })
    }

    fn on_wake(self: Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
    }

    fn wakes(&self) -> usize {
        self.wakes.load(Ordering::SeqCst)
    }
}

derive_arc_unsafe_wakeable!(Signal, on_wake);
declare_raw_waker_vtable!(Signal, SIGNAL_DECLARED);

/// A waker on the declared static, built as 0.1.4's `waker_from_arc(arc, &SIGNAL_DECLARED)` did.
fn waker_on_declared(signal: &Arc<Signal>) -> Waker {
    let data = Arc::into_raw(signal.clone()) as *const ();
    // SAFETY: `data` owns one strong ref of an Arc<Signal>, and SIGNAL_DECLARED is declared for Signal.
    unsafe { Waker::from_raw(RawWaker::new(data, &SIGNAL_DECLARED)) }
}

fn address(waker: &Waker) -> *const RawWakerVTable {
    black_box(waker.vtable() as *const RawWakerVTable)
}

#[test]
fn a_waker_on_the_declared_static_keeps_it_through_clones() {
    let signal = Signal::new();
    let waker = waker_on_declared(&signal);
    let declared = black_box(&SIGNAL_DECLARED as *const RawWakerVTable);
    assert_eq!(address(&waker), declared);

    let clones = [
        waker.clone(),
        Context::from_waker(&waker).waker().clone(),
        Context::from_waker(&waker.clone()).waker().clone(),
    ];
    for clone in &clones {
        assert_eq!(address(clone), declared, "a clone left the declared static");
        assert!(clone.will_wake(&waker));
        assert!(waker.will_wake(clone));
    }
    assert_eq!(Arc::strong_count(&signal), 5);

    drop((waker, clones));
    assert_eq!(Arc::strong_count(&signal), 1);
    assert_eq!(signal.wakes(), 0);
}

#[test]
fn a_waker_on_the_declared_static_wakes_and_counts_refs() {
    let signal = Signal::new();
    let waker = waker_on_declared(&signal);
    let clone = waker.clone();
    assert_eq!(Arc::strong_count(&signal), 3);

    waker.wake_by_ref();
    assert_eq!(signal.wakes(), 1);
    assert_eq!(Arc::strong_count(&signal), 3);

    clone.wake();
    assert_eq!(signal.wakes(), 2);
    assert_eq!(Arc::strong_count(&signal), 2);

    drop(waker);
    assert_eq!(Arc::strong_count(&signal), 1);
}
