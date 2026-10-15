//! The `# Safety` sections a caller of the `unsafe` task functions relies on, read from the source.
//! A contract is text, so these tests read the text: each one checks a clause the code needs and
//! an earlier draft (0.3.2, or 0.4.0 at e5833df) left out or got wrong. 0.3.1 has no
//! `from_raw_parts`, so they fail there too.

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

const TASK_FROM_RAW_PARTS: (&str, &str) = ("impl RegisteredTask {", "pub const unsafe fn from_raw_parts(");
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
    assert!(task.contains("never derived from a shared reference"), "RegisteredTask::from_raw_parts: {task}");

    let vtable = safety_of(VTABLE_FROM_RAW_PARTS.0, VTABLE_FROM_RAW_PARTS.1);
    assert!(vtable.contains("valid for reads and writes"), "UnsafeWakeableVTable::from_raw_parts: {vtable}");

    let ptr_poll = safety_of("impl UnsafeWakeablePtr {", "pub unsafe fn poll(");
    assert!(ptr_poll.contains("valid for reads and writes"), "UnsafeWakeablePtr::poll: {ptr_poll}");
    let ptr_drop = safety_of("impl UnsafeWakeablePtr {", "pub unsafe fn drop_in_place(");
    assert!(ptr_drop.starts_with("As [`UnsafeWakeablePtr::poll`]"), "UnsafeWakeablePtr::drop_in_place: {ptr_drop}");
}

/// The `# The data pointer` section of `UnsafeWakeablePtr`'s doc comment, as one line.
fn data_pointer_rule() -> String {
    let doc = docs_of("pub struct UnsafeWakeablePtr {", "pub struct UnsafeWakeablePtr {");
    let start = doc.find("# The data pointer\n").expect("UnsafeWakeablePtr has no `# The data pointer` section");
    let body = &doc[start + "# The data pointer\n".len()..];
    let end = body.find("\n# ").unwrap_or(body.len());
    body[..end].split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The data pointer must be valid for reads (and writes) as the type the vtable's functions read
/// it as. That allows the type itself, a `#[repr(transparent)]` wrapper of it, and a `#[repr(C)]`
/// struct whose first field is that type, which 0.1.4 ran under that type's key
/// (tests/repr_c_header.rs runs one). e5833df allowed only the first two. The rule is written once,
/// on `UnsafeWakeablePtr`, and every `# Safety` section that hands the pointer to a vtable function
/// states it and links there. Memory that only shares a size and alignment stays out: an `A(u8)`
/// holding 2 read as a `B(bool)` is Undefined Behavior, and so is writing through a pointer derived
/// from `&x`.
#[test]
fn the_data_pointer_rule_allows_a_repr_c_header_and_still_excludes_undefined_behavior() {
    let rule = data_pointer_rule();
    for allowed in [
        "valid for reads, and for `poll` and `drop_in_place` also for writes, as that type",
        "holding a valid value of it",
        "that type itself;",
        "a `#[repr(transparent)]` wrapper of it;",
        "a `#[repr(C)]` struct whose first field is that type.",
    ] {
        assert!(rule.contains(allowed), "the data pointer rule does not allow `{allowed}`: {rule}");
    }
    for excluded in [
        "a type that only has the same size and alignment: an `A(u8)` holding 2, read as a `B(bool)`, is Undefined Behavior",
        "a struct without `#[repr(C)]`, whose first field need not be at its start",
        "for writes, a pointer derived from a shared reference `&x`",
    ] {
        assert!(rule.contains(excluded), "the data pointer rule does not exclude `{excluded}`: {rule}");
    }

    let link = "(UnsafeWakeablePtr#the-data-pointer)";
    let sections = [
        ("RegisteredTask::from_raw_parts", safety_of(TASK_FROM_RAW_PARTS.0, TASK_FROM_RAW_PARTS.1), "valid for reads and writes as the type `ptr.vtable`'s functions read it as"),
        ("UnsafeWakeablePtr::poll", safety_of("impl UnsafeWakeablePtr {", "pub unsafe fn poll("), "valid for reads and writes as the type `self.vtable`'s functions read it as"),
        ("UnsafeWakeablePtr::wake", safety_of("impl UnsafeWakeablePtr {", "pub unsafe fn wake("), "valid for reads as the type `self.vtable`'s functions read it as"),
        ("UnsafeWakeableVTable::from_raw_parts", safety_of(VTABLE_FROM_RAW_PARTS.0, VTABLE_FROM_RAW_PARTS.1), "valid for reads and writes as a `T`"),
    ];
    for (name, text, requirement) in sections {
        assert!(text.contains(requirement), "{name} does not state `{requirement}`: {text}");
        assert!(text.contains(link), "{name} does not link the data pointer rule: {text}");
        assert!(!text.contains("points at a live task of the type"), "{name} still asks for the type itself: {text}");
        assert!(!text.contains("so that the memory always holds"), "{name} still lists only a transparent wrapper: {text}");
    }
    let readme = README.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        readme.contains("a `#[repr(C)]` struct whose first field is that type"),
        "the README's 0.4.0 section does not name the `#[repr(C)]` header case"
    );
    for text in [rule.as_str(), readme.as_str()] {
        assert!(!text.contains("same layout") && !text.contains("same-layout"), "still says same layout: {text}");
    }
}

/// `into_raw_parts` returns only the key and the pointer, and whether that pointer is a `Box<T>`
/// to free depends on how the task was made, so its docs and its `#[must_use]` text send the
/// caller to `owns_its_task` first. e5833df has neither.
#[test]
fn into_raw_parts_says_to_check_owns_its_task_first() {
    let docs = docs_of("impl RegisteredTask {", "pub fn into_raw_parts(");
    assert!(docs.contains("Check [`owns_its_task`](Self::owns_its_task) first"), "into_raw_parts: {docs}");

    let lines: Vec<&str> = LIB.lines().map(str::trim_start).collect();
    let at = lines.iter().position(|l| l.starts_with("pub fn into_raw_parts(")).expect("no into_raw_parts");
    let must_use = lines[..at].iter().rev().find(|l| l.starts_with("#[must_use")).expect("into_raw_parts is not #[must_use]");
    assert!(must_use.contains("check owns_its_task() before this call"), "into_raw_parts' must_use text: {must_use}");

    let owns = docs_of("impl RegisteredTask {", "pub fn owns_its_task(");
    assert!(owns.contains("Check it before [`into_raw_parts`](Self::into_raw_parts)"), "owns_its_task: {owns}");
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
