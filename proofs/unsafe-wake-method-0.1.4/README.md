# unsafe-wake-method-0.1.4

`waker_tables = "=0.1.4"`. `derive_arc_unsafe_wakeable!` given an `unsafe fn` as the wake method: `Option::unwrap_unchecked` reached through `Deref`, and the type's own `unsafe fn on_wake`.

`MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri run --bin deref_unwrap_unchecked` gives `error: Undefined Behavior: entering unreachable code`, at `<Empty as waker_tables::UnsafeWakeable>::wake` in `waker_tables-0.1.4/src/lib.rs:53:17`. `cargo build` gives no warnings, and the file has `#![forbid(unsafe_code)]`.

`cargo run --bin own_unsafe_fn` gives `unsafe on_wake ran, no unsafe at the call`.
