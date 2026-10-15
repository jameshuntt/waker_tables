# wrong-vtable-0.1.4

`waker_tables = "=0.1.4"`. An `Arc<Small>` handed to the safe `waker_from_arc` with `Big`'s vtable, in a file with `#![forbid(unsafe_code)]`.

`MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri run` gives `error: Undefined Behavior: constructing invalid value of type &alloc::sync::ArcInner<Big>: encountered a dangling reference (going beyond the bounds of its allocation)`, in `Big::on_wake`.
