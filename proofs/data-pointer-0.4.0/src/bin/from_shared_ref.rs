// A task pointer cast from a shared reference `&counter`, then `poll`, which writes through it.
use std::task::Poll;
use waker_tables::{derive_unsafe_wakeable_vtable, RegisteredTask, UnsafeWakeablePtr, UnsafeWakeableTask, VTableKey};

pub struct Counter(u32);
// SAFETY: the functions only touch `self`.
unsafe impl UnsafeWakeableTask for Counter {
    unsafe fn poll(&mut self) -> Poll<()> { self.0 += 1; Poll::Ready(()) }
    unsafe fn wake(&self) {}
    unsafe fn drop_in_place(&mut self) {}
}
derive_unsafe_wakeable_vtable!(Counter, COUNTER_VTABLE);

fn main() {
    let counter = Counter(0);
    let key = VTableKey { segment: "probe", kind: "counter", version: 1 };
    let ptr = UnsafeWakeablePtr { data: &counter as *const Counter as *mut (), vtable: &COUNTER_VTABLE };
    // SAFETY: wrong on purpose. The pointer is derived from `&counter`, and `poll` writes through it.
    let mut task = unsafe { RegisteredTask::from_raw_parts(key, ptr) };
    // SAFETY: Counter's functions assume nothing beyond exclusive access.
    let _ = unsafe { task.poll() };
    println!("counter = {}", counter.0);
}
