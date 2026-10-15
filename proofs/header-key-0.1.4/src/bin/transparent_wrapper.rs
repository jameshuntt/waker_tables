// A `#[repr(transparent)]` wrapper given to `RegisteredTask::new` under its inner type's key.
// Same source on 0.1.4 and 0.3.1.
use std::task::Poll;
use waker_tables::{derive_unsafe_wakeable_vtable, RegisteredTask, UnsafeWakeableTask, VTableKey, GLOBAL_VTABLE_REGISTRY};

pub struct Inner(u32);
// SAFETY: the functions only touch `self`.
unsafe impl UnsafeWakeableTask for Inner {
    unsafe fn poll(&mut self) -> Poll<()> { self.0 += 1; Poll::Pending }
    unsafe fn wake(&self) {}
    unsafe fn drop_in_place(&mut self) {}
}
derive_unsafe_wakeable_vtable!(Inner, INNER_VTABLE);

#[repr(transparent)]
pub struct Wrapped(Inner);
// SAFETY: each function forwards to Inner's.
unsafe impl UnsafeWakeableTask for Wrapped {
    unsafe fn poll(&mut self) -> Poll<()> { unsafe { self.0.poll() } }
    unsafe fn wake(&self) {}
    unsafe fn drop_in_place(&mut self) {}
}

fn main() {
    let key = VTableKey { segment: "probe", kind: "inner", version: 1 };
    GLOBAL_VTABLE_REGISTRY.register(key, &INNER_VTABLE);
    let mut task = RegisteredTask::new(Box::new(Wrapped(Inner(5))), key);
    // SAFETY: Inner's functions assume nothing beyond exclusive access.
    let polled = unsafe { task.poll() };
    println!("Wrapped under Inner's key: accepted, poll = {:?}", polled);
}
