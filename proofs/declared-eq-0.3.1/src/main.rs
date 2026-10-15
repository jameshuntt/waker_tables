// Does the static `declare_raw_waker_vtable!` declares equal the type's own vtable? 0.3.1's test
// `declared_static_is_the_types_own_vtable` (tests/raw_waker.rs @ 55f7fae) asserted that it does.
use std::sync::Arc;
use waker_tables::{declare_raw_waker_vtable, derive_arc_unsafe_wakeable, waker_from_arc, UnsafeWakeable};

pub struct Signal;
impl Signal { fn on_wake(self: Arc<Self>) {} }
derive_arc_unsafe_wakeable!(Signal, on_wake);
declare_raw_waker_vtable!(Signal, SIGNAL_VTABLE);

fn main() {
    let waker = waker_from_arc(Arc::new(Signal));
    println!(
        "*waker_from_arc(..).vtable() == SIGNAL_VTABLE: {}, *Signal::raw_waker_vtable() == SIGNAL_VTABLE: {}",
        *waker.vtable() == SIGNAL_VTABLE,
        *Signal::raw_waker_vtable() == SIGNAL_VTABLE
    );
}
