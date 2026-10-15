# unsafe-wake-method-0.3.0

`waker_tables = "=0.3.0"`. The same two programs as `unsafe-wake-method-0.1.4`, on the 0.2.0 API.

`MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri run --bin deref_unwrap_unchecked` gives `error: Undefined Behavior: entering unreachable code`, at `<Empty as waker_tables::UnsafeWakeable>::wake` in `waker_tables-0.3.0/src/lib.rs:122:17`. `cargo build` gives no warnings.

`cargo run --bin own_unsafe_fn` gives `unsafe on_wake ran, no unsafe at the call`.
