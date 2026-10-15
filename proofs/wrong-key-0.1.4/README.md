# wrong-key-0.1.4

`waker_tables = "=0.1.4"`. `wrong_key`: a `Box<Small>` given to `RegisteredTask::new` under `Big`'s key, then polled. `swap_data`: two tasks under their own keys whose `pub` data pointers are swapped in safe code, then polled.

`MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri run --bin wrong_key` gives `error: Undefined Behavior: constructing invalid value of type &mut Big: encountered a dangling reference (going beyond the bounds of its allocation)`, through `waker_tables::RegisteredTask::poll` at `waker_tables-0.1.4/src/lib.rs:371:18`.

`MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri run --bin swap_data` gives the same error, through the same `RegisteredTask::poll`.
