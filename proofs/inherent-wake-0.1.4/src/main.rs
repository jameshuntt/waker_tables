// A task type with an inherent `wake(&self)` next to `UnsafeWakeableTask::wake`: which one does
// the vtable from `derive_unsafe_wakeable_vtable!` run?
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::task::Poll;
use waker_tables::{derive_unsafe_wakeable_vtable, UnsafeWakeablePtr, UnsafeWakeableTask};

pub struct Task { inherent: AtomicUsize, trait_fn: AtomicUsize }
impl Task {
    fn wake(&self) { self.inherent.fetch_add(1, SeqCst); }
}
// SAFETY: the functions only touch `self`.
unsafe impl UnsafeWakeableTask for Task {
    unsafe fn poll(&mut self) -> Poll<()> { Poll::Ready(()) }
    unsafe fn wake(&self) { self.trait_fn.fetch_add(1, SeqCst); }
    unsafe fn drop_in_place(&mut self) {}
}
derive_unsafe_wakeable_vtable!(Task, TASK_VTABLE);

fn main() {
    let task = Task { inherent: AtomicUsize::new(0), trait_fn: AtomicUsize::new(0) };
    let ptr = UnsafeWakeablePtr { data: &task as *const Task as *mut (), vtable: &TASK_VTABLE };
    // SAFETY: `ptr` holds a live Task with its own vtable; wake only reads it.
    unsafe { ptr.wake() };
    println!("one wake through the vtable: inherent = {}, trait = {}", task.inherent.load(SeqCst), task.trait_fn.load(SeqCst));
    task.wake(); // the inherent one, called directly
}
