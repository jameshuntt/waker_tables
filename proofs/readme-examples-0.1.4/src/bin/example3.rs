// readme.md Example 3 from 0.1.4 (e5b534e), lines 204-251, unchanged.
use std::task::Poll;

use waker_tables::{
    VTableKey,
    GLOBAL_VTABLE_REGISTRY,
    RegisteredTask,
    UnsafeWakeableTask,
    derive_unsafe_wakeable_vtable,
};

struct EtlTask;
struct SearchTask;

unsafe impl UnsafeWakeableTask for EtlTask {
    unsafe fn poll(&mut self) -> Poll<()> { Poll::Ready(()) }
    unsafe fn wake(&self) {}
    unsafe fn drop_in_place(&mut self) {}
}

unsafe impl UnsafeWakeableTask for SearchTask {
    unsafe fn poll(&mut self) -> Poll<()> { Poll::Pending }
    unsafe fn wake(&self) {}
    unsafe fn drop_in_place(&mut self) {}
}

derive_unsafe_wakeable_vtable!(EtlTask, ETL_VTABLE);
derive_unsafe_wakeable_vtable!(SearchTask, SEARCH_VTABLE);

fn main() {
    let etl_key = VTableKey { segment: "etl", kind: "parse", version: 1 };
    let search_key = VTableKey { segment: "search", kind: "merge", version: 1 };

    GLOBAL_VTABLE_REGISTRY.register(etl_key, &ETL_VTABLE);
    GLOBAL_VTABLE_REGISTRY.register(search_key, &SEARCH_VTABLE);

    let etl = RegisteredTask::new(Box::new(EtlTask), etl_key);
    let mut search = RegisteredTask::new(Box::new(SearchTask), search_key);

    unsafe {
        // etl: ready
        let _ = etl.ptr.vtable.poll_fn(etl.ptr.data);

        // search: pending
        let _ = search.poll();
        search.wake();
        search.drop_in_place();
    }
}
