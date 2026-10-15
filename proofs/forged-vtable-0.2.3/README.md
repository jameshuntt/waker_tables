# forged-vtable-0.2.3

`waker_tables = "=0.2.3"`. A task vtable with `Big`'s functions and `Small`'s `type_id`, written as a struct literal with no `unsafe`, registered, given a `Box<Small>`, polled once.

`MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri run` gives `error: Undefined Behavior: constructing invalid value of type &mut Big: encountered a dangling reference (going beyond the bounds of its allocation)`, in `waker_tables::task_poll::<Big>` at `waker_tables-0.2.3/src/lib.rs:310:14`.
