// A waker and five clones, as tests/raw_waker.rs's `clone_adds_one_ref_and_drop_takes_it_back`.
// 0.1.4 builds the waker on the static `declare_raw_waker_vtable!` declares.
use std::sync::Arc;
use waker_tables::{declare_raw_waker_vtable, derive_arc_unsafe_wakeable, waker_from_arc};

pub struct Signal;
impl Signal { fn on_wake(self: Arc<Self>) {} }
derive_arc_unsafe_wakeable!(Signal, on_wake);
declare_raw_waker_vtable!(Signal, SIGNAL_VTABLE);

fn main() {
    let waker = waker_from_arc(Arc::new(Signal), &SIGNAL_VTABLE);
    let clones: Vec<_> = (0..5).map(|_| waker.clone()).collect();
    let n = clones.iter().filter(|c| c.will_wake(&waker)).count();
    println!("will_wake true for {n} of 5 clones");
}
