// `derive_arc_unsafe_wakeable!` on an unsized type.
use std::sync::Arc;
use waker_tables::derive_arc_unsafe_wakeable;

#[allow(dead_code)]
pub struct Dst([u8]);
impl Dst { fn on_wake(self: Arc<Self>) {} }
derive_arc_unsafe_wakeable!(Dst, on_wake);

fn main() {
    println!("derive_arc_unsafe_wakeable!(Dst, on_wake) built");
}
