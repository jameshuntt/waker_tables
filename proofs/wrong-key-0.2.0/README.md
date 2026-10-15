# wrong-key-0.2.0

`waker_tables = "=0.2.0"`. The `wrong_key` program from `wrong-key-0.1.4`.

`cargo run --bin wrong_key` gives a panic in `RegisteredTask::new`: `vtable for key VTableKey { segment: "probe", kind: "big", version: 1 } is for wrong_key::Big, not wrong_key::Small`.
