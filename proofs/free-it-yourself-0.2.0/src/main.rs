// 0.1.4's `RegisteredTask` never freed its task, so code that did not want a leak freed the box
// itself through the `pub` field. Here it is moved to 0.2.0 with the one rename
// the changelog gives, `ptr` -> `ptr()`.
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::task::Poll;
use waker_tables::{derive_unsafe_wakeable_vtable, RegisteredTask, UnsafeWakeableTask, VTableKey, GLOBAL_VTABLE_REGISTRY};

static DROPS: AtomicUsize = AtomicUsize::new(0);
pub struct Job(u32);
impl Drop for Job { fn drop(&mut self) { DROPS.fetch_add(1, SeqCst); } }
// SAFETY: the functions only touch `self`.
unsafe impl UnsafeWakeableTask for Job {
    unsafe fn poll(&mut self) -> Poll<()> { self.0 += 1; Poll::Ready(()) }
    unsafe fn wake(&self) {}
    unsafe fn drop_in_place(&mut self) {}
}
derive_unsafe_wakeable_vtable!(Job, JOB_VTABLE);

fn main() {
    let key = VTableKey { segment: "probe", kind: "job", version: 1 };
    GLOBAL_VTABLE_REGISTRY.register(key, &JOB_VTABLE);
    let mut task = RegisteredTask::new(Box::new(Job(0)), key);
    // SAFETY: Job's functions assume nothing beyond exclusive access; the box came from `new`.
    unsafe {
        let _ = task.poll();
        task.drop_in_place();
        drop(Box::from_raw(task.ptr().data as *mut Job));
    }
    drop(task);
    println!("Job dropped {} time(s)", DROPS.load(SeqCst));
}
