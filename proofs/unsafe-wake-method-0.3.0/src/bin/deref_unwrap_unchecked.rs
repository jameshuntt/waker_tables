#![forbid(unsafe_code)]
// No `unsafe` in this file. `Option::unwrap_unchecked` is an `unsafe fn`, reached through `Deref`.
use std::ops::Deref;
use std::sync::Arc;
use waker_tables::{derive_arc_unsafe_wakeable, waker_from_arc};

pub struct Empty;
impl Deref for Empty {
    type Target = Option<u8>;
    fn deref(&self) -> &Option<u8> { &None }
}
derive_arc_unsafe_wakeable!(Empty, unwrap_unchecked);

fn main() {
    waker_from_arc(Arc::new(Empty)).wake();
}
