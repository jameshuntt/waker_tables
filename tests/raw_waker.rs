//! The `Waker` half: `derive_arc_unsafe_wakeable!`, `declare_raw_waker_vtable!`, `waker_from_arc`,
//! `SmartWakerPtr`. Every test checks the `Arc` strong count, so a missing or extra ref fails here
//! and a leak fails under Miri.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::thread;

use waker_tables::{declare_raw_waker_vtable, derive_arc_unsafe_wakeable, raw_waker_from_ptr, waker_from_arc, SmartWakerPtr};

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
declare_raw_waker_vtable!(Signal, SIGNAL_VTABLE);

#[test]
fn wake_consumes_the_waker_and_its_ref() {
    let signal = Signal::new();
    let waker = waker_from_arc(signal.clone());
    assert_eq!(Arc::strong_count(&signal), 2);

    waker.wake();
    assert_eq!(signal.wakes(), 1);
    assert_eq!(Arc::strong_count(&signal), 1);
}

#[test]
fn wake_by_ref_keeps_the_waker_and_its_ref() {
    let signal = Signal::new();
    let waker = waker_from_arc(signal.clone());

    waker.wake_by_ref();
    waker.wake_by_ref();
    assert_eq!(signal.wakes(), 2);
    assert_eq!(Arc::strong_count(&signal), 2);

    drop(waker);
    assert_eq!(Arc::strong_count(&signal), 1);
}

#[test]
fn clone_adds_one_ref_and_drop_takes_it_back() {
    let signal = Signal::new();
    let waker = waker_from_arc(signal.clone());
    let clones: Vec<_> = (0..5).map(|_| waker.clone()).collect();
    assert_eq!(Arc::strong_count(&signal), 7);

    for clone in &clones {
        assert!(clone.will_wake(&waker));
    }
    drop(clones);
    assert_eq!(Arc::strong_count(&signal), 2);

    drop(waker);
    assert_eq!(Arc::strong_count(&signal), 1);
    assert_eq!(signal.wakes(), 0);
}

#[test]
fn dropping_an_unwoken_waker_frees_its_ref() {
    let signal = Signal::new();
    drop(waker_from_arc(signal.clone()));
    assert_eq!(Arc::strong_count(&signal), 1);
    assert_eq!(signal.wakes(), 0);
}

#[test]
fn the_last_ref_can_live_in_the_waker() {
    let waker = waker_from_arc(Signal::new());
    let clone = waker.clone();
    clone.wake();
    waker.wake_by_ref();
    drop(waker);
}

#[test]
fn wakers_cross_threads() {
    let signal = Signal::new();
    let waker = waker_from_arc(signal.clone());

    let handles: Vec<_> = (0..4)
        .map(|_| {
            let waker = waker.clone();
            thread::spawn(move || {
                waker.wake_by_ref();
                waker.wake();
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }

    assert_eq!(signal.wakes(), 8);
    drop(waker);
    assert_eq!(Arc::strong_count(&signal), 1);
}

/// Pending until woken once, then ready: the waker a real executor hands a future.
struct WakeOnce {
    polled: bool,
}

impl Future for WakeOnce {
    type Output = &'static str;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.polled {
            Poll::Ready("done")
        } else {
            self.polled = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

#[test]
fn drives_a_future() {
    let signal = Signal::new();
    let waker = waker_from_arc(signal.clone());
    let mut cx = Context::from_waker(&waker);
    let mut future = WakeOnce { polled: false };

    assert_eq!(Pin::new(&mut future).poll(&mut cx), Poll::Pending);
    assert_eq!(signal.wakes(), 1);
    assert_eq!(Pin::new(&mut future).poll(&mut cx), Poll::Ready("done"));
    assert_eq!(signal.wakes(), 1);

    drop(waker);
    assert_eq!(Arc::strong_count(&signal), 1);
}

#[test]
fn smart_waker_ptr_round_trip() {
    let signal = Signal::new();
    let ptr = SmartWakerPtr::from_arc(signal.clone());
    assert_eq!(Arc::strong_count(&signal), 2);

    let cloned = ptr.to_cloned_arc();
    assert!(Arc::ptr_eq(&cloned, &signal));
    assert_eq!(Arc::strong_count(&signal), 3);
    drop(cloned);

    let back = ptr.to_arc();
    assert!(Arc::ptr_eq(&back, &signal));
    drop(back);
    assert_eq!(Arc::strong_count(&signal), 1);
}

#[test]
fn smart_waker_ptr_drop_gives_the_ref_back() {
    let signal = Signal::new();
    let ptr = SmartWakerPtr::from_arc(signal.clone());
    assert_eq!(Arc::strong_count(&signal), 2);
    drop(ptr);
    assert_eq!(Arc::strong_count(&signal), 1);
}

#[test]
fn declared_static_is_the_types_own_vtable() {
    let signal = Signal::new();
    let raw = raw_waker_from_ptr(Arc::into_raw(signal.clone()));
    // SAFETY: `raw` carries a strong ref of an Arc<Signal> and Signal's own vtable.
    let waker = unsafe { std::task::Waker::from_raw(raw) };
    assert_eq!(*waker.vtable(), SIGNAL_VTABLE);
    assert_eq!(*waker_from_arc(signal.clone()).vtable(), SIGNAL_VTABLE);
    waker.wake();
    assert_eq!(signal.wakes(), 1);
    assert_eq!(Arc::strong_count(&signal), 1);
}

/// A second waker type, so a test can show each type keeps its own functions.
struct Other {
    hits: AtomicUsize,
}

impl Other {
    fn on_wake(self: Arc<Self>) {
        self.hits.fetch_add(10, Ordering::SeqCst);
    }
}

derive_arc_unsafe_wakeable!(Other, on_wake);

#[test]
fn each_type_runs_its_own_functions() {
    let signal = Signal::new();
    let other = Arc::new(Other { hits: AtomicUsize::new(0) });
    let a = waker_from_arc(signal.clone());
    let b = waker_from_arc(other.clone());
    assert!(!a.will_wake(&b));

    a.wake_by_ref();
    b.wake_by_ref();
    assert_eq!(signal.wakes(), 1);
    assert_eq!(other.hits.load(Ordering::SeqCst), 10);

    drop((a, b));
    assert_eq!(Arc::strong_count(&signal), 1);
    assert_eq!(Arc::strong_count(&other), 1);
}
