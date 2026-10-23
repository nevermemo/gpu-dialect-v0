use super::{changed_fixtures, outcome, with_stat_result};

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

#[test]
fn a_failed_diff_listing_is_reported_beside_the_bless_outcome() {
    let changed = ["loops.slang".to_owned()];
    let blessed = outcome(false, &changed);
    // A good listing leaves the outcome alone.
    assert_eq!(with_stat_result(blessed.clone(), Ok(())), blessed);
    assert!(with_stat_result(Ok(()), Ok(())).is_ok());
    // A failed listing keeps the bless message and adds its own.
    let both =
        with_stat_result(blessed, Err("git diff --stat failed (exit 128)".to_owned())).unwrap_err();
    assert!(both.contains("golden blessed"), "{both}");
    assert!(both.contains("loops.slang"), "{both}");
    assert!(both.contains("git diff --stat failed (exit 128)"), "{both}");
    // Even a passing outcome becomes an error when the listing fails.
    assert!(with_stat_result(Ok(()), Err("boom".to_owned())).is_err());
}
