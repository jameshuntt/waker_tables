// A `SmartWakerPtr` dropped without `to_arc`.
use std::sync::Arc;
use waker_tables::{derive_arc_unsafe_wakeable, SmartWakerPtr};

pub struct Signal;
impl Signal { fn on_wake(self: Arc<Self>) {} }
derive_arc_unsafe_wakeable!(Signal, on_wake);

fn main() {
    let signal = Arc::new(Signal);
    drop(SmartWakerPtr::from_arc(signal.clone()));
    println!("strong_count after dropping a SmartWakerPtr = {}", Arc::strong_count(&signal));
}
