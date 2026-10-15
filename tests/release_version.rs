//! `declare_raw_waker_vtable!` went back to 0.1.4's form, and code that built on 0.3.1 can break:
//! two uses in one module now define `__waker_clone` (and the other three helpers) twice (E0428),
//! and the declared static no longer compares equal to `T::raw_waker_vtable()`. A Cargo
//! requirement of `"0.3"` would take a 0.3.x patch release, so this ships as 0.4.0.

#[test]
fn the_declare_raw_waker_vtable_change_is_not_a_0_3_patch_release() {
    let major: u64 = env!("CARGO_PKG_VERSION_MAJOR").parse().unwrap();
    let minor: u64 = env!("CARGO_PKG_VERSION_MINOR").parse().unwrap();
    assert!((major, minor) >= (0, 4), "version {} would reach crates on waker_tables 0.3", env!("CARGO_PKG_VERSION"));

    let readme = include_str!("../readme.md");
    assert!(readme.contains("\n## 0.4.0\n"), "the README has no 0.4.0 section");
    assert!(!readme.contains("\n## 0.3.2\n"), "the README still has a 0.3.2 section; 0.3.2 was never released");
    assert!(!readme.contains("waker_tables = \"0.3\""), "the README's quick start still asks for 0.3");
}
