# wake-by-ref-names-0.3.1

`waker_tables = "=0.3.1"`. `own_clone_ptr`: a type with its own safe `clone_ptr`, `drop_ptr` and `wake_by_ref`, no `unsafe` in the file. `wake_named_wake`: the wake method is named `wake`.

`MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri run --bin own_clone_ptr` gives `strong_count after wake_by_ref: 1` (it was 2), then `error: Undefined Behavior: constructing invalid value of type &alloc::sync::ArcInner<Node>: encountered a dangling reference (use-after-free)` at the next `Arc::strong_count`. Plain `cargo run --bin own_clone_ptr` prints `1` and then `0`.

`cargo build --bin wake_named_wake` gives `error[E0308]: mismatched types`, ``expected `Arc<Task>`, found `*const Task` ``, and ends ``error: could not compile `proof-wake-by-ref-names-0-3-1` (bin "wake_named_wake") due to 1 previous error``.
