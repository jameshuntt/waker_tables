# readme-examples-0.1.4

`waker_tables = "=0.1.4"`. Examples 2, 3 and 4 of the 0.1.4 README (`readme.md` at `e5b534e`), copied unchanged.

`MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri run --bin example2` gives `error: memory leaked: alloc269 (Rust heap, size: 8, align: 8)`, allocated at `Box::new(CounterTask { polls: AtomicUsize::new(0) })`.

`cargo build --bin example3` gives ``error[E0599]: no method named `poll_fn` found for reference `&'static UnsafeWakeableVTable` in the current scope``, at `etl.ptr.vtable.poll_fn(etl.ptr.data)`.

`MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri run --bin example4` gives `error: memory leaked: alloc297 (Rust heap, size: 4, align: 4)`, allocated at `Box::new(TickTask { ticks_left: 3 })`.
