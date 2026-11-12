use std::process::Command;

#[test]
fn long_content_and_project_filters_preserve_browser_column_geometry() {
    let output = tempfile::tempdir().unwrap();
    let result = Command::new("node")
        .arg("scripts/check-ui-layout.mjs")
        .arg(output.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "浏览器布局回归失败：\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["cases"], 20);
}

#[test]
fn transfer_feedback_waits_for_receipts_and_preserves_interaction() {
    let output = tempfile::tempdir().unwrap();
    let result = Command::new("node")
        .arg("scripts/check-transfer-feedback.mjs")
        .arg(output.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "等待反馈回归失败：\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["cases"], 28);
}
