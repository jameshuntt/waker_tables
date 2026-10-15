// An `A(u8)` holding 2, handed to `B(bool)`'s vtable through `RegisteredTask::from_raw_parts`.
// Same size and alignment, but 2 is not a valid `bool`. "The data pointer" rules this out.
use std::task::Poll;
use waker_tables::{derive_unsafe_wakeable_vtable, RegisteredTask, UnsafeWakeablePtr, UnsafeWakeableTask, VTableKey};

#[allow(dead_code)]
pub struct A(u8);
pub struct B(bool);
// SAFETY: the functions only touch `self`.
unsafe impl UnsafeWakeableTask for B {
    unsafe fn poll(&mut self) -> Poll<()> { if self.0 { Poll::Ready(()) } else { Poll::Pending } }
    unsafe fn wake(&self) {}
    unsafe fn drop_in_place(&mut self) {}
}
derive_unsafe_wakeable_vtable!(B, B_VTABLE);

fn main() {
    assert_eq!((size_of::<A>(), align_of::<A>()), (size_of::<B>(), align_of::<B>()));
    let mut a = A(2);
    let key = VTableKey { segment: "probe", kind: "b", version: 1 };
    let ptr = UnsafeWakeablePtr { data: &mut a as *mut A as *mut (), vtable: &B_VTABLE };
    // SAFETY: wrong on purpose. A has B's size and alignment, but its memory does not hold a valid B.
    let mut task = unsafe { RegisteredTask::from_raw_parts(key, ptr) };
    // SAFETY: B's functions assume nothing beyond exclusive access.
    let _ = unsafe { task.poll() };
}
