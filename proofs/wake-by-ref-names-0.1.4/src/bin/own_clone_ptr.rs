// A type with its own safe `clone_ptr`, `drop_ptr` and `wake_by_ref`, all wrong for a waker.
// No `unsafe` in this file. One `wake_by_ref` must leave the strong count where it was.
#![forbid(unsafe_code)]
use std::sync::Arc;
use waker_tables::{declare_raw_waker_vtable, derive_arc_unsafe_wakeable, waker_from_arc};

pub struct Node;
#[allow(dead_code)]
impl Node {
    fn clone_ptr(p: *const Self) -> *const Self { p }
    fn drop_ptr(_p: *const Self) {}
    fn wake_by_ref(_p: *const Self) {}
    fn on_wake(self: Arc<Self>) {}
}
derive_arc_unsafe_wakeable!(Node, on_wake);
declare_raw_waker_vtable!(Node, NODE_VTABLE);

fn main() {
    let node = Arc::new(Node);
    let waker = waker_from_arc(node.clone(), &NODE_VTABLE);
    println!("strong_count before wake_by_ref: {}", Arc::strong_count(&node));
    waker.wake_by_ref();
    println!("strong_count after wake_by_ref: {}", Arc::strong_count(&node));
    drop(waker);
    println!("strong_count after dropping the waker: {}", Arc::strong_count(&node));
}
