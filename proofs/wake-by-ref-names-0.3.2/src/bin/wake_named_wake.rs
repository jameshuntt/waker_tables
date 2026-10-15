// The wake method is named `wake`.
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use waker_tables::{derive_arc_unsafe_wakeable, waker_from_arc};

pub struct Task { wakes: AtomicUsize }
impl Task {
    fn wake(self: Arc<Self>) { self.wakes.fetch_add(1, Ordering::SeqCst); }
}
derive_arc_unsafe_wakeable!(Task, wake);

fn main() {
    let task = Arc::new(Task { wakes: AtomicUsize::new(0) });
    let waker = waker_from_arc(task.clone());
    waker.wake_by_ref();
    waker.wake();
    println!("wakes = {}, strong_count = {}", task.wakes.load(Ordering::SeqCst), Arc::strong_count(&task));
}
