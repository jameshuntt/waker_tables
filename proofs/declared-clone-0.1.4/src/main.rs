// A waker built on the static `declare_raw_waker_vtable!` declares (std's `Waker::from_raw`, as
// 0.1.4's `waker_from_arc(arc, &DECLARED)` did): which vtable does its clone carry?
use std::sync::Arc;
use std::task::{RawWaker, Waker};
use waker_tables::{declare_raw_waker_vtable, derive_arc_unsafe_wakeable};

pub struct Signal;
impl Signal { fn on_wake(self: Arc<Self>) {} }
derive_arc_unsafe_wakeable!(Signal, on_wake);
declare_raw_waker_vtable!(Signal, DECLARED);

fn main() {
    let data = Arc::into_raw(Arc::new(Signal)) as *const ();
    // SAFETY: `data` owns one strong ref of an Arc<Signal>, and DECLARED is declared for Signal.
    let waker = unsafe { Waker::from_raw(RawWaker::new(data, &DECLARED)) };
    let clone = waker.clone();
    println!(
        "clone carries DECLARED: {}, clone.will_wake(&waker): {}",
        std::ptr::eq(clone.vtable(), &DECLARED),
        clone.will_wake(&waker)
    );
}
