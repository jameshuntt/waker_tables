# mixed-vtable-0.1.4

`waker_tables = "=0.1.4"`. A task vtable built in safe code from the `pub` fields of two others: `Big`'s poll with `Small`'s wake and drop. `RegisteredTask::new` takes it for a `Box<Small>`, and the task is polled once.

`MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri run` gives `error: Undefined Behavior: constructing invalid value of type &mut Big: encountered a dangling reference (going beyond the bounds of its allocation)`, in `BIG_VTABLE::{closure#0}` at `waker_tables-0.1.4/src/lib.rs:250:37`.
