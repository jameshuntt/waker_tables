// Two `declare_raw_waker_vtable!` in one module.
use std::sync::Arc;
use waker_tables::{declare_raw_waker_vtable, derive_arc_unsafe_wakeable};

pub struct A;
impl A { fn on_wake(self: Arc<Self>) {} }
derive_arc_unsafe_wakeable!(A, on_wake);
pub struct B;
impl B { fn on_wake(self: Arc<Self>) {} }
derive_arc_unsafe_wakeable!(B, on_wake);

declare_raw_waker_vtable!(A, A_VTABLE);
declare_raw_waker_vtable!(B, B_VTABLE);

fn main() {
    println!("two declare_raw_waker_vtable! in one module built");
}
