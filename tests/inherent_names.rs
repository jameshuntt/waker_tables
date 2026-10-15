//! The derive macro's generated code must call the trait's functions, never a type's own functions
//! with the same names. `Self::wake` and `Self::clone_ptr` resolve to inherent functions first, so
//! 0.3.1 and the 0.4.0 draft used a type's own `clone_ptr` inside `wake_by_ref`: a use-after-free with
//! no `unsafe` in the user's code. Miri runs this file in CI.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use waker_tables::{derive_arc_unsafe_wakeable, waker_from_arc};

/// Has its own `clone_ptr`, `drop_ptr` and `wake_by_ref`, all safe and all wrong for a waker.
struct Node {
    wakes: AtomicUsize,
}

#[allow(dead_code)]
impl Node {
    fn clone_ptr(p: *const Self) -> *const Self {
        p
    }
    fn drop_ptr(_p: *const Self) {}
    fn wake_by_ref(_p: *const Self) {}
    fn on_wake(self: Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
    }
}

derive_arc_unsafe_wakeable!(Node, on_wake);

#[test]
fn a_types_own_clone_ptr_is_not_used_by_wake_by_ref() {
    let node = Arc::new(Node { wakes: AtomicUsize::new(0) });
    let waker = waker_from_arc(node.clone());
    assert_eq!(Arc::strong_count(&node), 2);
    waker.wake_by_ref();
    assert_eq!(node.wakes.load(Ordering::SeqCst), 1);
    assert_eq!(Arc::strong_count(&node), 2, "wake_by_ref keeps the waker's ref");
    let clone = waker.clone();
    assert_eq!(Arc::strong_count(&node), 3);
    drop(clone);
    drop(waker);
    assert_eq!(Arc::strong_count(&node), 1);
}

/// Its wake method is named `wake`, as 0.1.4 allowed.
struct Task {
    wakes: AtomicUsize,
}

impl Task {
    fn wake(self: Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
    }
}

derive_arc_unsafe_wakeable!(Task, wake);

#[test]
fn a_wake_method_named_wake_works() {
    let task = Arc::new(Task { wakes: AtomicUsize::new(0) });
    let waker = waker_from_arc(task.clone());
    waker.wake_by_ref();
    waker.wake();
    assert_eq!(task.wakes.load(Ordering::SeqCst), 2);
    assert_eq!(Arc::strong_count(&task), 1);
}
