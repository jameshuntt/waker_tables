// Two `#[repr(C)]` task types whose first field is `Header`, both given to `RegisteredTask::new`
// under the header's key. Same source on 0.1.4 and 0.3.1.
use std::task::Poll;
use waker_tables::{derive_unsafe_wakeable_vtable, RegisteredTask, UnsafeWakeableTask, VTableKey, GLOBAL_VTABLE_REGISTRY};

pub struct Header { polls: u32 }
// SAFETY: the functions only touch `self`.
unsafe impl UnsafeWakeableTask for Header {
    unsafe fn poll(&mut self) -> Poll<()> { self.polls += 1; Poll::Pending }
    unsafe fn wake(&self) {}
    unsafe fn drop_in_place(&mut self) {}
}
derive_unsafe_wakeable_vtable!(Header, HEADER_VTABLE);

#[repr(C)]
pub struct Fetch { header: Header, url: String }
#[repr(C)]
pub struct Parse { header: Header, bytes: Vec<u8> }

// Never called through a vtable: they exist so `RegisteredTask::new` accepts the types.
// SAFETY: the functions only touch `self`.
unsafe impl UnsafeWakeableTask for Fetch {
    unsafe fn poll(&mut self) -> Poll<()> { Poll::Ready(()) }
    unsafe fn wake(&self) {}
    unsafe fn drop_in_place(&mut self) {}
}
// SAFETY: as above.
unsafe impl UnsafeWakeableTask for Parse {
    unsafe fn poll(&mut self) -> Poll<()> { Poll::Ready(()) }
    unsafe fn wake(&self) {}
    unsafe fn drop_in_place(&mut self) {}
}

fn main() {
    let key = VTableKey { segment: "probe", kind: "header", version: 1 };
    GLOBAL_VTABLE_REGISTRY.register(key, &HEADER_VTABLE);
    let mut fetch = RegisteredTask::new(Box::new(Fetch { header: Header { polls: 0 }, url: "u".into() }), key);
    let mut parse = RegisteredTask::new(Box::new(Parse { header: Header { polls: 0 }, bytes: vec![1, 2] }), key);
    // SAFETY: Header's functions assume nothing beyond exclusive access.
    let polled = unsafe { (fetch.poll(), parse.poll()) };
    println!("Fetch and Parse under Header's key: accepted, polls = {:?}", polled);
}
