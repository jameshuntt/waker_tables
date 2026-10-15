// Two `RawWakerVTable`s for one type, chosen per waker: `HIGH` counts in `high`, the declared
// `LOW` runs the type's own wake, which counts in `low`. Same source on 0.1.4 and 0.3.1.
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::task::{RawWaker, RawWakerVTable, Waker};
use waker_tables::{declare_raw_waker_vtable, derive_arc_unsafe_wakeable, raw_waker_from_ptr, waker_from_arc};

pub struct Job { high: AtomicUsize, low: AtomicUsize }
impl Job { fn on_wake(self: Arc<Self>) { self.low.fetch_add(1, SeqCst); } }
derive_arc_unsafe_wakeable!(Job, on_wake);
declare_raw_waker_vtable!(Job, LOW);

// SAFETY (all four): a waker on HIGH carries the `*const Job` of a live Arc<Job> owning one ref.
unsafe fn high_clone(p: *const ()) -> RawWaker { unsafe { Arc::increment_strong_count(p as *const Job) }; RawWaker::new(p, &HIGH) }
unsafe fn high_wake(p: *const ()) { let job = unsafe { Arc::from_raw(p as *const Job) }; job.high.fetch_add(1, SeqCst); }
unsafe fn high_wake_by_ref(p: *const ()) { unsafe { &*(p as *const Job) }.high.fetch_add(1, SeqCst); }
unsafe fn high_drop(p: *const ()) { drop(unsafe { Arc::from_raw(p as *const Job) }) }
static HIGH: RawWakerVTable = RawWakerVTable::new(high_clone, high_wake, high_wake_by_ref, high_drop);

fn main() {
    let job = Arc::new(Job { high: AtomicUsize::new(0), low: AtomicUsize::new(0) });
    waker_from_arc(job.clone(), &HIGH).wake();
    waker_from_arc(job.clone(), &LOW).wake();
    // SAFETY: the RawWaker owns one strong ref of an Arc<Job> and carries HIGH.
    let waker = unsafe { Waker::from_raw(raw_waker_from_ptr(Arc::into_raw(job.clone()), &HIGH)) };
    waker.wake_by_ref();
    drop(waker);
    println!("high = {}, low = {}, strong_count = {}", job.high.load(SeqCst), job.low.load(SeqCst), Arc::strong_count(&job));
}
