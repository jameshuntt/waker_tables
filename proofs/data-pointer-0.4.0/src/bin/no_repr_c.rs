// A task that starts with a `Header` field but has no `#[repr(C)]`, run under the header's vtable.
// Without `#[repr(C)]` the compiler may put another field first, and here it does.
use std::mem::offset_of;
use std::task::Poll;
use waker_tables::{derive_unsafe_wakeable_vtable, RegisteredTask, UnsafeWakeablePtr, UnsafeWakeableTask, VTableKey};

pub struct Header { done: bool }
// SAFETY: the functions only touch `self`.
unsafe impl UnsafeWakeableTask for Header {
    unsafe fn poll(&mut self) -> Poll<()> { if self.done { Poll::Ready(()) } else { Poll::Pending } }
    unsafe fn wake(&self) {}
    unsafe fn drop_in_place(&mut self) {}
}
derive_unsafe_wakeable_vtable!(Header, HEADER_VTABLE);

pub struct Fetch { header: Header, id: u64 } // no #[repr(C)]

fn main() {
    println!("offset_of!(Fetch, header) = {}", offset_of!(Fetch, header));
    let fetch = Box::into_raw(Box::new(Fetch { header: Header { done: false }, id: 2 }));
    let key = VTableKey { segment: "probe", kind: "header", version: 1 };
    let ptr = UnsafeWakeablePtr { data: fetch as *mut (), vtable: &HEADER_VTABLE };
    // SAFETY: wrong on purpose. Fetch has no #[repr(C)], so its first byte need not be its header.
    let mut task = unsafe { RegisteredTask::from_raw_parts(key, ptr) };
    // SAFETY: Header's functions assume nothing beyond exclusive access.
    let _ = unsafe { task.poll() };
    drop(task);
    // SAFETY: `fetch` came from Box::into_raw above and is freed once.
    let fetch = unsafe { Box::from_raw(fetch) };
    println!("id = {}", fetch.id);
}
