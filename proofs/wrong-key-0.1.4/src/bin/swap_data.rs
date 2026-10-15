// Two tasks, each under its own correct key. Safe code swaps their `pub` data pointers, then
// each is polled once.
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
derive_unsafe_wakeable_vtable!(Small, SMALL_VTABLE);
derive_unsafe_wakeable_vtable!(Big, BIG_VTABLE);

fn main() {
    let small_key = VTableKey { segment: "probe", kind: "small", version: 1 };
    let big_key = VTableKey { segment: "probe", kind: "big", version: 1 };
    GLOBAL_VTABLE_REGISTRY.register(small_key, &SMALL_VTABLE);
    GLOBAL_VTABLE_REGISTRY.register(big_key, &BIG_VTABLE);
    let mut small = RegisteredTask::new(Box::new(Small(0)), small_key);
    let mut big = RegisteredTask::new(Box::new(Big([0; 8])), big_key);
    std::mem::swap(&mut small.ptr.data, &mut big.ptr.data); // safe: the fields are pub
    // SAFETY: RegisteredTask::poll's documented contract is exclusive access, which `&mut` gives.
    unsafe {
        let _ = small.poll();
        let _ = big.poll();
    }
}
