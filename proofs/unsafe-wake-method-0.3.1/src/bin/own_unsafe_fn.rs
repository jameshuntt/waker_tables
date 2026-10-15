// The wake method is the type's own `unsafe fn`; nothing below calls it inside `unsafe`.
use std::sync::Arc;
use waker_tables::{derive_arc_unsafe_wakeable, waker_from_arc};

pub struct OnExecutorThread;
impl OnExecutorThread {
    /// # Safety
    /// Only on the executor thread.
    unsafe fn on_wake(self: Arc<Self>) { println!("unsafe on_wake ran, no unsafe at the call"); }
}
derive_arc_unsafe_wakeable!(OnExecutorThread, on_wake);

fn main() { waker_from_arc(Arc::new(OnExecutorThread)).wake(); }
