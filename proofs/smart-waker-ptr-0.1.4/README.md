# smart-waker-ptr-0.1.4

`waker_tables = "=0.1.4"`. A `SmartWakerPtr` dropped without `to_arc`.

`cargo run` gives `strong_count after dropping a SmartWakerPtr = 2`. Under Miri (`MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri run`) the run ends with `error: memory leaked`.
