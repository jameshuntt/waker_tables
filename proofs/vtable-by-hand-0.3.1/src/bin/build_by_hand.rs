// A task vtable written by hand as a struct literal: logging around `Job`'s own poll. Same source
// on 0.1.4 and 0.3.1.
use std::task::Poll;
use waker_tables::{UnsafeWakeablePtr, UnsafeWakeableTask, UnsafeWakeableVTable};

pub struct Job(u32);
// SAFETY: the functions only touch `self`.
unsafe impl UnsafeWakeableTask for Job {
    unsafe fn poll(&mut self) -> Poll<()> { self.0 += 1; Poll::Pending }
    unsafe fn wake(&self) {}
    unsafe fn drop_in_place(&mut self) {}
}
// SAFETY (all three): called through UnsafeWakeablePtr on a live, exclusively held Job.
unsafe fn logged_poll(p: *mut ()) -> Poll<()> { println!("poll"); unsafe { (*(p as *mut Job)).poll() } }
unsafe fn logged_wake(_: *const ()) { println!("wake") }
unsafe fn logged_drop(_: *mut ()) { println!("drop") }
static LOGGED: UnsafeWakeableVTable = UnsafeWakeableVTable { poll_fn: logged_poll, wake_fn: logged_wake, drop_fn: logged_drop };

fn main() {
    let data = Box::into_raw(Box::new(Job(0)));
    let mut ptr = UnsafeWakeablePtr { data: data as *mut (), vtable: &LOGGED };
    // SAFETY: `ptr` holds a live Job from Box::into_raw, used only here, and freed once at the end.
    unsafe {
        let _ = ptr.poll();
        ptr.wake();
        ptr.drop_in_place();
        drop(Box::from_raw(data));
    }
    println!("hand-built vtable ran");
}
