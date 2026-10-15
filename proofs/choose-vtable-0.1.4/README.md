# choose-vtable-0.1.4

`waker_tables = "=0.1.4"`. Two `RawWakerVTable`s for one type, picked per waker: `HIGH` counts in `high`, the declared `LOW` runs the type's own wake, which counts in `low`.

`cargo run` gives `high = 2, low = 1, strong_count = 1`. Miri (`MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri run`) reports nothing.
