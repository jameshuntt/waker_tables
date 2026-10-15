// A `Box<Small>` given to `RegisteredTask::new` under a key registered for `Big`, then one `poll`.
use std::task::Poll;
use waker_tables::{derive_unsafe_wakeable_vtable, RegisteredTask, UnsafeWakeableTask, VTableKey, GLOBAL_VTABLE_REGISTRY};

pub struct Small(u8);
pub struct Big([u64; 8]);

// SAFETY: touches only self.
unsafe impl UnsafeWakeableTask for Small {
    unsafe fn poll(&mut self) -> Poll<()> { self.0 += 1; Poll::Ready(()) }
    unsafe fn wake(&self) {}
    unsafe fn drop_in_place(&mut self) {}
}
// SAFETY: touches only self.
unsafe impl UnsafeWakeableTask for Big {
    unsafe fn poll(&mut self) -> Poll<()> { self.0[7] += 1; Poll::Ready(()) }
    unsafe fn wake(&self) {}
    unsafe fn drop_in_place(&mut self) {}
}
derive_unsafe_wakeable_vtable!(Big, BIG_VTABLE);

fn main() {
    let key = VTableKey { segment: "probe", kind: "big", version: 1 };
    GLOBAL_VTABLE_REGISTRY.register(key, &BIG_VTABLE);
    let mut task = RegisteredTask::new(Box::new(Small(0)), key);
    // SAFETY: RegisteredTask::poll's documented contract is exclusive access, which `&mut task` gives.
    let _ = unsafe { task.poll() };
}
