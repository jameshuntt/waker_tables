// readme.md Example 2 from 0.1.4 (e5b534e), lines 147-192, unchanged.
use std::task::Poll;
use std::sync::atomic::{AtomicUsize, Ordering};

use waker_tables::{
    UnsafeWakeableTask,
    UnsafeWakeablePtr,
    derive_unsafe_wakeable_vtable,
};

struct CounterTask {
    polls: AtomicUsize,
}

unsafe impl UnsafeWakeableTask for CounterTask {
    unsafe fn poll(&mut self) -> Poll<()> {
        self.polls.fetch_add(1, Ordering::Relaxed);
        Poll::Pending
    }

    unsafe fn wake(&self) {
        // In a real scheduler: enqueue / notify / etc.
    }

    unsafe fn drop_in_place(&mut self) {
        // If you had manual resources, you'd clean them here.
    }
}

// Generate a static vtable for the type.
derive_unsafe_wakeable_vtable!(CounterTask, COUNTER_VTABLE);

fn main() {
    let boxed = Box::new(CounterTask { polls: AtomicUsize::new(0) });

    // Erase the type behind (data ptr + vtable).
    let mut ptr = UnsafeWakeablePtr {
        data: Box::into_raw(boxed) as *mut (),
        vtable: &COUNTER_VTABLE,
    };

    unsafe {
        let _ = ptr.poll();
        ptr.wake();
        ptr.drop_in_place();
    }
}
