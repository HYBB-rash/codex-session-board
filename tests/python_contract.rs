use anyhow::Result;
use notion_program::integrity::assert_content_equal;

#[test]
fn matches_python_content_comparison_contract() -> Result<()> {
    let cases: Vec<(String, String, bool)> =
        serde_json::from_str(include_str!("fixtures/content-comparison.json"))?;
    for (index, (expected, actual, accepted)) in cases.into_iter().enumerate() {
        let outcome = assert_content_equal(&expected, &actual, "Python compatibility");
        assert_eq!(
            outcome.is_ok(),
            accepted,
            "case {index}: {expected:?} -> {actual:?}: {outcome:?}"
        );
    }
    Ok(())
}
