// A `RegisteredTask` written as a struct literal over a slot of a fixed array (not a Box), run, and
// its pointer moved back out. Same source on 0.1.4 and 0.3.1.
use std::task::Poll;
use waker_tables::{derive_unsafe_wakeable_vtable, RegisteredTask, UnsafeWakeablePtr, UnsafeWakeableTask, VTableKey};

pub struct Job(u32);
// SAFETY: the functions only touch `self`.
unsafe impl UnsafeWakeableTask for Job {
    unsafe fn poll(&mut self) -> Poll<()> { self.0 += 1; Poll::Pending }
    unsafe fn wake(&self) {}
    unsafe fn drop_in_place(&mut self) {}
}
derive_unsafe_wakeable_vtable!(Job, JOB_VTABLE);

fn main() {
    let mut slab = [Job(0), Job(10)];
    let key = VTableKey { segment: "probe", kind: "slab", version: 1 };
    let slot = &mut slab[1] as *mut Job as *mut ();
    let mut task = RegisteredTask { key, ptr: UnsafeWakeablePtr { data: slot, vtable: &JOB_VTABLE } };
    // SAFETY: the pointer is a live Job in `slab`, used only through `task` meanwhile.
    let _ = unsafe { task.ptr.poll() };
    let moved: UnsafeWakeablePtr = task.ptr; // the pointer moved back out
    println!("slab[1] = {}, the pointer moved back out is slab[1]: {}", slab[1].0, moved.data == slot);
}
