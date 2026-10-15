//! An executor that runs five jobs with at most two in flight. The limit is uh-tom-mick's
//! `AtomicAdmission`; a job that finds the gate full parks its waker, and a finishing job
//! wakes the next one. The executor's run queue is fed by wakers from `waker_from_arc`.
//! At the end the gate is shut down and must report drained.
//!
//!   cargo run --example admission_executor

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use uh_tom_mick::{AdmissionError, AdmissionGuard, AtomicAdmission, AtomicCounter, DrainStatus};
use waker_tables::{derive_arc_unsafe_wakeable, waker_from_arc};

const JOBS: usize = 5;
const MAX_IN_FLIGHT: u32 = 2;

/// A job's waker: puts the job's index back on the run queue.
struct JobWaker {
    job: usize,
    queue: Arc<Mutex<VecDeque<usize>>>,
}

impl JobWaker {
    fn on_wake(self: Arc<Self>) {
        self.queue.lock().unwrap().push_back(self.job);
    }
}

derive_arc_unsafe_wakeable!(JobWaker, on_wake);

/// Everything the jobs share.
struct Shared {
    gate: AtomicAdmission,
    parked: RefCell<VecDeque<Waker>>,
    completed: AtomicCounter,
    peak: RefCell<u32>,
}

/// Ready with a guard once the gate admits; parks the waker while the gate is full.
struct Admit<'a> {
    shared: &'a Shared,
}

impl<'a> Future for Admit<'a> {
    type Output = AdmissionGuard<'a>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.shared.gate.try_enter() {
            Ok(guard) => Poll::Ready(guard),
            Err(AdmissionError::AtCapacity { .. }) => {
                self.shared.parked.borrow_mut().push_back(cx.waker().clone());
                Poll::Pending
            }
            Err(other) => panic!("gate refused a job: {other}"),
        }
    }
}

/// Pending once: gives the other jobs a turn while this one holds its slot.
struct YieldOnce(bool);

impl Future for YieldOnce {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            return Poll::Ready(());
        }
        self.0 = true;
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

async fn job(id: usize, shared: &Shared) {
    let guard = Admit { shared }.await;
    let active = shared.gate.active();
    println!("job {id} admitted ({active}/{MAX_IN_FLIGHT} in flight)");
    {
        let mut peak = shared.peak.borrow_mut();
        *peak = (*peak).max(active);
    }

    YieldOnce(false).await;

    shared.completed.increment().unwrap();
    drop(guard);
    println!("job {id} done");
    if let Some(next) = shared.parked.borrow_mut().pop_front() {
        next.wake();
    }
}

fn main() {
    let shared = Shared {
        gate: AtomicAdmission::new(MAX_IN_FLIGHT).unwrap(),
        parked: RefCell::new(VecDeque::new()),
        completed: AtomicCounter::new(0),
        peak: RefCell::new(0),
    };
    let queue = Arc::new(Mutex::new((0..JOBS).collect::<VecDeque<_>>()));

    let mut jobs: Vec<Option<Pin<Box<dyn Future<Output = ()> + '_>>>> =
        (0..JOBS).map(|id| Some(Box::pin(job(id, &shared)) as Pin<Box<dyn Future<Output = ()>>>)).collect();
    let wakers: Vec<Waker> = (0..JOBS)
        .map(|job| waker_from_arc(Arc::new(JobWaker { job, queue: queue.clone() })))
        .collect();

    loop {
        let next = queue.lock().unwrap().pop_front();
        let Some(id) = next else { break };
        let Some(future) = jobs[id].as_mut() else { continue };
        if future.as_mut().poll(&mut Context::from_waker(&wakers[id])).is_ready() {
            jobs[id] = None;
        }
    }

    assert!(jobs.iter().all(Option::is_none), "every job finished");
    assert_eq!(shared.completed.load(), JOBS as u64);
    assert_eq!(*shared.peak.borrow(), MAX_IN_FLIGHT, "never more than the limit in flight");

    shared.gate.begin_shutdown();
    assert_eq!(shared.gate.drain_status(), DrainStatus::Drained);
    println!(
        "{} jobs done, at most {} in flight, gate drained",
        shared.completed.load(),
        shared.peak.borrow()
    );

    drop(jobs);
    drop(wakers);
    assert_eq!(Arc::strong_count(&queue), 1, "every waker gave its ref back");
}
