# wake-by-ref-names-0.3.2

`waker_tables = "=0.3.2"`. `own_clone_ptr`: a type with its own safe `clone_ptr`, `drop_ptr` and `wake_by_ref`, no `unsafe` in the file. `wake_named_wake`: the wake method is named `wake`.

`MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri run --bin own_clone_ptr` gives `strong_count before wake_by_ref: 2`, `strong_count after wake_by_ref: 2`, `strong_count after dropping the waker: 1`, and nothing from Miri.

`MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri run --bin wake_named_wake` gives `wakes = 2, strong_count = 1`, and nothing from Miri.
