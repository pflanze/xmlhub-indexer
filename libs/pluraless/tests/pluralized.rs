use pluraless::pluralized;

// This can't be in `pluraless/src/lib.rs` or $crate aka `pluralized`
// would not be found. Uh.

// Also, tests/*.rs must have `#[test]`, `main` is *not* run. Uh.

#[test]
fn t_pluralized() {
    let t = |n| {
        pluralized! {n => these}
        these
    };
    assert_eq!(t(0), "these");
    assert_eq!(t(1), "this");
    assert_eq!(t(2), "these");

    let t = |n| {
        pluralized! {n => these, patterns, subscriptions}
        (these, subscriptions, patterns)
    };
    assert_eq!(t(0), ("these", "subscriptions", "patterns"));
    assert_eq!(t(1), ("this", "subscription", "pattern"));
}
