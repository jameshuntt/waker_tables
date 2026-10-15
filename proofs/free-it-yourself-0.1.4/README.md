# free-it-yourself-0.1.4

`waker_tables = "=0.1.4"`. 0.1.4's `RegisteredTask` never freed its task, so this frees the box itself through the `pub` field, then drops the `RegisteredTask`.

`cargo run` gives `Job dropped 1 time(s)`. Miri (`MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri run`) reports nothing.
