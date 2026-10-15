//! A whole executor on `waker_from_arc`: `block_on` parks the thread until the future's
//! waker unparks it. The future here is finished by a timer thread.
//!
//!   cargo run --example block_on

use std::future::Future;
use std::pin::pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::thread::{self, Thread};
use std::time::Duration;

use waker_tables::{derive_arc_unsafe_wakeable, waker_from_arc};

/// The waker: unparks the thread that is blocked in `block_on`, and counts its wakes.
struct Unparker {
    thread: Thread,
    wakes: AtomicUsize,
}

impl Unparker {
    fn on_wake(self: Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
        self.thread.unpark();
    }
}

derive_arc_unsafe_wakeable!(Unparker, on_wake);

fn block_on<F: Future>(future: F) -> (F::Output, usize) {
    let unparker = Arc::new(Unparker { thread: thread::current(), wakes: AtomicUsize::new(0) });
    let waker = waker_from_arc(unparker.clone());
    let mut cx = Context::from_waker(&waker);
    let mut future = pin!(future);

    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
            return (output, unparker.wakes.load(Ordering::SeqCst));
        }
        thread::park();
    }
}

/// Ready once a timer thread has fired `ticks` times; each tick wakes the last waker it saw.
struct Ticks {
    shared: Arc<Mutex<TickState>>,
}

struct TickState {
    fired: u32,
    waker: Option<Waker>,
}

impl Ticks {
    fn start(ticks: u32, every: Duration) -> Self {
        let shared = Arc::new(Mutex::new(TickState { fired: 0, waker: None }));
        let timer = shared.clone();
        thread::spawn(move || {
            for _ in 0..ticks {
                thread::sleep(every);
                let mut state = timer.lock().unwrap();
                state.fired += 1;
                if let Some(waker) = state.waker.take() {
                    waker.wake();
                }
            }
        });
        Self { shared }
    }
}

impl Future for Ticks {
    type Output = u32;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u32> {
        let mut state = self.shared.lock().unwrap();
        if state.fired == 3 {
            Poll::Ready(state.fired)
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

fn main() {
    let (fired, wakes) = block_on(Ticks::start(3, Duration::from_millis(20)));
    println!("timer fired {fired} times; the waker woke the executor {wakes} times");
    assert_eq!(fired, 3);
    assert!(wakes >= 1);
}
