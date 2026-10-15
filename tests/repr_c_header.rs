//! Tasks that start with a `#[repr(C)]` header, run under the header's key through the header's
//! vtable: two task types share one vtable and one key. 0.1.4's `RegisteredTask::new` did not
//! check the type, so such tasks ran there. `new` now refuses them, and
//! `RegisteredTask::from_raw_parts` (`unsafe`) takes them: its `# Safety` asks for memory valid
//! for reads and writes as the type the vtable's functions read it as, and a `#[repr(C)]` struct
//! whose first field is that type is such memory. CI runs this under Miri.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::Poll;

use waker_tables::{
    derive_unsafe_wakeable_vtable, RegisteredTask, UnsafeWakeablePtr, UnsafeWakeableTask, VTableKey,
    GLOBAL_VTABLE_REGISTRY,
};

/// What the scheduler reads: the first field of every task below.
struct Header {
    ready_after: u32,
    polls: u32,
    wakes: AtomicUsize,
    cleaned: bool,
}

impl Header {
    fn new(ready_after: u32) -> Self {
        Self { ready_after, polls: 0, wakes: AtomicUsize::new(0), cleaned: false }
    }
}

// SAFETY: the functions only touch `self`.
unsafe impl UnsafeWakeableTask for Header {
    unsafe fn poll(&mut self) -> Poll<()> {
        self.polls += 1;
        if self.polls >= self.ready_after { Poll::Ready(()) } else { Poll::Pending }
    }

    unsafe fn wake(&self) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
    }

    unsafe fn drop_in_place(&mut self) {
        self.cleaned = true;
    }
}

derive_unsafe_wakeable_vtable!(Header, HEADER_VTABLE);

/// `#[repr(C)]`, so `header` is at offset 0 and the rest of the task follows it.
#[repr(C)]
struct Fetch {
    header: Header,
    url: String,
    drops: Arc<AtomicUsize>,
}

/// A second task type under the same key.
#[repr(C)]
struct Parse {
    header: Header,
    bytes: Vec<u8>,
    drops: Arc<AtomicUsize>,
}

impl Drop for Fetch {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

impl Drop for Parse {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

const HEADER_KEY: VTableKey = VTableKey { segment: "repr_c_header", kind: "header", version: 1 };

#[test]
fn tasks_that_start_with_a_header_run_under_the_headers_key() {
    GLOBAL_VTABLE_REGISTRY.register(HEADER_KEY, &HEADER_VTABLE);
    let drops = Arc::new(AtomicUsize::new(0));
    let vtable = GLOBAL_VTABLE_REGISTRY.get(&HEADER_KEY).unwrap();
    assert!(vtable.is_for::<Header>() && !vtable.is_for::<Fetch>() && !vtable.is_for::<Parse>());

    let fetch = Box::into_raw(Box::new(Fetch { header: Header::new(2), url: "u".into(), drops: drops.clone() }));
    let parse = Box::into_raw(Box::new(Parse { header: Header::new(3), bytes: vec![1, 2], drops: drops.clone() }));

    // SAFETY: each pointer is a live `#[repr(C)]` struct whose first field is a Header, from
    // Box::into_raw, so it is valid for reads and writes as a Header; it stays valid until freed
    // below, after its task is gone, and is used only through its task meanwhile.
    let tasks = unsafe {
        [
            RegisteredTask::from_raw_parts(HEADER_KEY, UnsafeWakeablePtr { data: fetch as *mut (), vtable }),
            RegisteredTask::from_raw_parts(HEADER_KEY, UnsafeWakeablePtr { data: parse as *mut (), vtable }),
        ]
    };

    let mut back = Vec::new();
    for mut task in tasks {
        assert_eq!(task.key(), HEADER_KEY);
        // SAFETY: Header's functions assume nothing beyond exclusive access.
        unsafe {
            task.wake();
            while task.poll().is_pending() {}
            task.drop_in_place();
        }
        back.push(task.into_raw_parts().1);
    }
    assert_eq!(drops.load(Ordering::SeqCst), 0, "nothing was freed");
    assert_eq!((back[0].data, back[1].data), (fetch as *mut (), parse as *mut ()));

    // SAFETY: each pointer is the Box from above, freed once, as the type it was boxed as.
    let (fetch, parse) = unsafe { (Box::from_raw(fetch), Box::from_raw(parse)) };
    let fetch_header = (fetch.header.polls, fetch.header.wakes.load(Ordering::SeqCst), fetch.header.cleaned);
    let parse_header = (parse.header.polls, parse.header.wakes.load(Ordering::SeqCst), parse.header.cleaned);
    assert_eq!(fetch_header, (2, 1, true));
    assert_eq!(parse_header, (3, 1, true));
    assert_eq!(fetch.url, "u", "writes through the header left the fields after it alone");
    assert_eq!(parse.bytes, [1, 2]);
    drop((fetch, parse));
    assert_eq!(drops.load(Ordering::SeqCst), 2);
}
