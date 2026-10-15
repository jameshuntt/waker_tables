// `declare_raw_waker_vtable!` and the helper fns it may leave at the call site.
use std::sync::Arc;
use waker_tables::{declare_raw_waker_vtable, derive_arc_unsafe_wakeable};

pub struct Signal;
impl Signal { fn on_wake(self: Arc<Self>) {} }
derive_arc_unsafe_wakeable!(Signal, on_wake);
declare_raw_waker_vtable!(Signal, DECLARED);

fn main() {
    let signal = Arc::new(Signal);
    let data = Arc::into_raw(signal.clone()) as *const ();
    // SAFETY: `data` owns one strong ref of an Arc<Signal>; `__waker_drop` gives it up.
    unsafe { __waker_drop(data) };
    println!("called __waker_drop; strong_count = {}", Arc::strong_count(&signal));
}
