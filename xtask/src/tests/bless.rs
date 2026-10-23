use super::{changed_fixtures, outcome};

fn snapshot(items: &[(&str, &str)]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|(name, hash)| ((*name).to_owned(), (*hash).to_owned()))
        .collect()
}

#[test]
fn changed_added_and_removed_fixtures_are_listed_once() {
    let before = snapshot(&[("a.slang", "1"), ("b.slang", "2"), ("c.slang", "3")]);
    let after = snapshot(&[("a.slang", "1"), ("b.slang", "9"), ("d.slang", "4")]);
    assert_eq!(
        changed_fixtures(&before, &after),
        ["b.slang", "c.slang", "d.slang"]
    );
    assert!(changed_fixtures(&before, &before).is_empty());
}

#[test]
fn only_an_unchanged_passing_run_succeeds() {
    assert!(outcome(true, &[]).is_ok());
    let changed = ["loops.slang".to_owned()];
    let blessed = outcome(false, &changed).unwrap_err();
    assert!(blessed.contains("golden blessed"), "{blessed}");
    assert!(blessed.contains("loops.slang"), "{blessed}");
    assert!(blessed.contains("without BLESS_GOLDEN"), "{blessed}");
    assert!(outcome(true, &changed).is_err());
    let failed = outcome(false, &[]).unwrap_err();
    assert!(failed.contains("real failure"), "{failed}");
}
