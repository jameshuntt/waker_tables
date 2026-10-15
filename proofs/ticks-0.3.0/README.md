# ticks-0.3.0

`waker_tables = "=0.3.0"`. `examples/block_on.rs` from 0.3.0 (`c0b70f3`) unchanged, with the test `ticks_returns_its_own_count` from 0.3.1 (`c907695`) added.

`cargo test` gives ``assertion `left == right` failed``, `left: [None, None, None, Some(3), Some(3), Some(3)]`, `right: [Some(0), Some(1), Some(2), Some(3), Some(4), Some(7)]`.
