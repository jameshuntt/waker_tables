#![forbid(unsafe_code)]
// An `Arc<Small>` handed to `waker_from_arc` with `Big`'s vtable. No `unsafe` in this file.
use std::sync::Arc;
use waker_tables::{derive_arc_unsafe_wakeable, waker_from_arc};

pub struct Small(pub u8);
impl Small { fn on_wake(self: Arc<Self>) { println!("small {}", self.0); } }
derive_arc_unsafe_wakeable!(Small, on_wake);

pub struct Big(pub [u64; 8]);
impl Big { fn on_wake(self: Arc<Self>) { println!("big {}", self.0[7]); } }
derive_arc_unsafe_wakeable!(Big, on_wake);

mod big_vt {
    waker_tables::declare_raw_waker_vtable!(super::Big, BIG_VTABLE);
}

fn main() {
    // Big's on_wake reads 64 bytes from a 1-byte value.
    waker_from_arc(Arc::new(Small(1)), &big_vt::BIG_VTABLE).wake();
}
