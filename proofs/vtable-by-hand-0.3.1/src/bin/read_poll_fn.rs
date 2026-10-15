// A task vtable's poll function read out and called directly. Same source on 0.1.4 and 0.3.1.
use std::task::Poll;
use waker_tables::{derive_unsafe_wakeable_vtable, UnsafeWakeableTask};

pub struct Job(u32);
// SAFETY: the functions only touch `self`.
unsafe impl UnsafeWakeableTask for Job {
    unsafe fn poll(&mut self) -> Poll<()> { self.0 += 1; Poll::Pending }
    unsafe fn wake(&self) {}
    unsafe fn drop_in_place(&mut self) {}
}
derive_unsafe_wakeable_vtable!(Job, JOB_VTABLE);

fn main() {
    let data = Box::into_raw(Box::new(Job(0)));
    // SAFETY: `data` is a live Job from Box::into_raw, used only here, and freed once.
    let polled = unsafe { (JOB_VTABLE.poll_fn)(data as *mut ()) };
    let job = unsafe { Box::from_raw(data) };
    println!("direct poll_fn call: {:?}, polls = {}", polled, job.0);
}
