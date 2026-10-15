//! The `# Safety` sections a caller of the `unsafe` task functions relies on, read from the source.
//! A contract is text, so these tests read the text: each one checks a clause the code needs and
//! the 0.3.2 draft left out or got wrong. 0.3.1 has no `from_raw_parts`, so they fail there too.

const LIB: &str = include_str!("../src/lib.rs");
const README: &str = include_str!("../readme.md");

/// The doc comment of the first item at or after the line starting with `within` whose line
/// starts with `signature` (both after indentation), as text.
fn docs_of(within: &str, signature: &str) -> String {
    let lines: Vec<&str> = LIB.lines().collect();
    let from = lines
        .iter()
        .position(|l| l.trim_start().starts_with(within))
        .unwrap_or_else(|| panic!("no `{within}` in src/lib.rs"));
    let at = from
        + lines[from..]
            .iter()
            .position(|l| l.trim_start().starts_with(signature))
            .unwrap_or_else(|| panic!("no `{signature}` after `{within}` in src/lib.rs"));
    let mut doc: Vec<&str> = lines[..at]
        .iter()
        .rev()
        .map(|l| l.trim_start())
        .skip_while(|l| l.starts_with("#["))
        .take_while(|l| l.starts_with("///"))
        .map(|l| l.trim_start_matches("///").trim())
        .collect();
    doc.reverse();
    doc.join("\n")
}

/// The `# Safety` section of that doc comment, up to the next heading or code block, as one line.
fn safety_of(within: &str, signature: &str) -> String {
    let doc = docs_of(within, signature);
    let start = doc
        .find("# Safety\n")
        .unwrap_or_else(|| panic!("`{signature}` has no `# Safety` section"));
    let body = &doc[start + "# Safety\n".len()..];
    let end = [body.find("\n#"), body.find("\n```")].into_iter().flatten().min().unwrap_or(body.len());
    body[..end].split_whitespace().collect::<Vec<_>>().join(" ")
}

const TASK_FROM_RAW_PARTS: (&str, &str) = ("impl RegisteredTask {", "pub unsafe fn from_raw_parts(");
const VTABLE_FROM_RAW_PARTS: (&str, &str) = ("impl UnsafeWakeableVTable {", "pub const unsafe fn from_raw_parts<");

/// The six callers of a vtable's functions: (impl, signature, the vtable function, `T`'s trait fn).
const CALLERS: [(&str, &str, &str, &str); 6] = [
    ("impl UnsafeWakeablePtr {", "pub unsafe fn poll(", "`poll` function", "UnsafeWakeableTask::poll"),
    ("impl UnsafeWakeablePtr {", "pub unsafe fn wake(", "`wake` function", "UnsafeWakeableTask::wake"),
    ("impl UnsafeWakeablePtr {", "pub unsafe fn drop_in_place(", "`drop` function", "UnsafeWakeableTask::drop_in_place"),
    ("impl RegisteredTask {", "pub unsafe fn poll(", "`poll` function", "UnsafeWakeableTask::poll"),
    ("impl RegisteredTask {", "pub unsafe fn wake(", "`wake` function", "UnsafeWakeableTask::wake"),
    ("impl RegisteredTask {", "pub unsafe fn drop_in_place(", "`drop` function", "UnsafeWakeableTask::drop_in_place"),
];

/// `poll` and `drop_in_place` write through the task pointer (the derived `poll` takes `&mut T`),
/// so a pointer cast from `&x` meets a contract without this clause and is Undefined Behavior.
#[test]
fn the_task_pointer_must_be_valid_for_writes() {
    let task = safety_of(TASK_FROM_RAW_PARTS.0, TASK_FROM_RAW_PARTS.1);
    assert!(task.contains("valid for reads and writes"), "RegisteredTask::from_raw_parts: {task}");
    assert!(task.contains("never cast from a shared reference"), "RegisteredTask::from_raw_parts: {task}");

    let vtable = safety_of(VTABLE_FROM_RAW_PARTS.0, VTABLE_FROM_RAW_PARTS.1);
    assert!(vtable.contains("valid for reads and writes"), "UnsafeWakeableVTable::from_raw_parts: {vtable}");

    let ptr_poll = safety_of("impl UnsafeWakeablePtr {", "pub unsafe fn poll(");
    assert!(ptr_poll.contains("valid for reads and writes"), "UnsafeWakeablePtr::poll: {ptr_poll}");
    let ptr_drop = safety_of("impl UnsafeWakeablePtr {", "pub unsafe fn drop_in_place(");
    assert!(ptr_drop.starts_with("As [`UnsafeWakeablePtr::poll`]"), "UnsafeWakeablePtr::drop_in_place: {ptr_drop}");
}

/// The same size and alignment do not make a value valid as the vtable's type: an `A(u8)` holding
/// 2 read as a `B(bool)` is Undefined Behavior. Only a `#[repr(transparent)]` wrapper of the type
/// always holds a valid value of it.
#[test]
fn only_a_transparent_wrapper_may_stand_in_for_the_vtables_type() {
    let task = safety_of(TASK_FROM_RAW_PARTS.0, TASK_FROM_RAW_PARTS.1);
    assert!(
        task.contains("a `#[repr(transparent)]` wrapper of it, so that the memory always holds a valid value of that type"),
        "RegisteredTask::from_raw_parts: {task}"
    );
    for text in [task.as_str(), README] {
        assert!(!text.contains("same layout") && !text.contains("same-layout"), "still says same layout: {text}");
    }
}

/// With a vtable from `UnsafeWakeableVTable::from_raw_parts`, the function that runs is the one
/// passed in, not `T`'s trait function, so the callers must uphold that function's contract.
#[test]
fn callers_uphold_the_vtables_function_not_the_tasks_trait_function() {
    for (within, signature, function, trait_fn) in CALLERS {
        let text = safety_of(within, signature);
        let name = format!("{within} {signature}");
        assert!(text.contains(&format!("the vtable's {function}")), "{name}: {text}");
        assert!(text.contains(trait_fn), "{name}: {text}");
        assert!(text.contains("UnsafeWakeableVTable::from_raw_parts"), "{name}: {text}");
        assert!(!text.contains("task's own"), "{name} still points at the task's own contract: {text}");
    }
}
