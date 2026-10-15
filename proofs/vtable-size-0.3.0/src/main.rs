// The size of a task vtable and of the pointer pair, and what `{:?}` prints for each.
use std::mem::size_of;
use std::task::Poll;
use waker_tables::{derive_unsafe_wakeable_vtable, UnsafeWakeablePtr, UnsafeWakeableTask, UnsafeWakeableVTable};

pub struct Inner(u32);
// SAFETY: the functions only touch `self`.
unsafe impl UnsafeWakeableTask for Inner {
    unsafe fn poll(&mut self) -> Poll<()> { self.0 += 1; Poll::Pending }
    unsafe fn wake(&self) {}
    unsafe fn drop_in_place(&mut self) {}
}
derive_unsafe_wakeable_vtable!(Inner, INNER_VTABLE);

fn main() {
    println!(
        "size_of UnsafeWakeableVTable = {}, UnsafeWakeablePtr = {}",
        size_of::<UnsafeWakeableVTable>(),
        size_of::<UnsafeWakeablePtr>()
    );
    println!("{:?}", &INNER_VTABLE);
    println!("{:?}", UnsafeWakeablePtr { data: std::ptr::null_mut(), vtable: &INNER_VTABLE });
}
