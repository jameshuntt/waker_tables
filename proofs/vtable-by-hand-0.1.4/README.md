# vtable-by-hand-0.1.4

`waker_tables = "=0.1.4"`. `build_by_hand`: a task vtable written as a struct literal, logging around the task's own poll. `read_poll_fn`: a vtable's poll function read out of its field and called.

`cargo run --bin build_by_hand` gives `poll`, `wake`, `drop`, `hand-built vtable ran`. Miri (`MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri run --bin build_by_hand`) reports nothing.

`cargo run --bin read_poll_fn` gives `direct poll_fn call: Pending, polls = 1`. Miri reports nothing.
