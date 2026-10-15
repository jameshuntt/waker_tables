// A waker and five clones, as tests/raw_waker.rs's `clone_adds_one_ref_and_drop_takes_it_back`.
use std::sync::Arc;
use waker_tables::{derive_arc_unsafe_wakeable, waker_from_arc};

pub struct Signal;
impl Signal { fn on_wake(self: Arc<Self>) {} }
derive_arc_unsafe_wakeable!(Signal, on_wake);

fn main() {
    let waker = waker_from_arc(Arc::new(Signal));
    let clones: Vec<_> = (0..5).map(|_| waker.clone()).collect();
    let n = clones.iter().filter(|c| c.will_wake(&waker)).count();
    println!("will_wake true for {n} of 5 clones");
}
