# header-key-0.3.1

`waker_tables = "=0.3.1"`. The same source as `header-key-0.1.4`.

`cargo run --bin transparent_wrapper` gives a panic in `RegisteredTask::new`: `vtable for key VTableKey { segment: "probe", kind: "inner", version: 1 } is for transparent_wrapper::Inner, not transparent_wrapper::Wrapped`.

`cargo run --bin repr_c_header` gives a panic: `vtable for key VTableKey { segment: "probe", kind: "header", version: 1 } is for repr_c_header::Header, not repr_c_header::Fetch`.
