use std::process::Command;

#[test]
fn host_catalog_filters_preserve_cached_rows_and_isolated_native_drafts() {
    let result = Command::new("node")
        .arg("scripts/check-remote-catalog.mjs")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "目录面板回归失败：\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["cases"], 67);
    assert_eq!(report["cleanup"]["groupEmpty"], true);
    assert_eq!(report["cleanup"]["profileRemoved"], true);
}

#[test]
fn catalog_load_waits_for_latest_snapshot_before_native_creation() {
    let result = Command::new("node")
        .arg("scripts/check-catalog-load-wait.mjs")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "目录快照加载交错回归失败：\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["passed"], true);
    assert_eq!(report["cleanup"]["groupEmpty"], true);
    assert_eq!(report["cleanup"]["profileRemoved"], true);
}

#[test]
fn mcp_bridge_preserves_routes_results_and_host_theme() {
    let result = Command::new("node")
        .arg("scripts/check-app-bridge.mjs")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "离线桥接回归失败：\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["cases"], 57);
    assert_eq!(report["cleanup"]["groupEmpty"], true);
    assert_eq!(report["cleanup"]["profileRemoved"], true);
}

#[test]
fn reply_preview_preserves_scroll_focus_and_rejects_late_bodies() {
    let build = Command::new("node").args(["scripts/build-app-bridge.mjs", "--check", "--offline"]).output().unwrap();
    assert!(build.status.success(), "预览类型或离线产物不一致：\n{}\n{}",
        String::from_utf8_lossy(&build.stdout),String::from_utf8_lossy(&build.stderr));
    let output = tempfile::tempdir().unwrap();
    let resource = output.path().join("mcp-resource.html");
    std::fs::write(&resource, codex_session_board::mcp::resource()["text"].as_str().unwrap()).unwrap();
    let result = Command::new("node").arg("scripts/check-reply-preview.mjs")
        .arg(output.path()).arg(resource).output().unwrap();
    assert!(result.status.success(), "答复预览失败：\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),String::from_utf8_lossy(&result.stderr));
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["passed"], true);
    assert_eq!(report["cleanup"]["profileRemoved"], true);
}

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
    assert_eq!(report["cases"], 48);
    let filters = Command::new("node")
        .arg("scripts/check-host-filter.mjs")
        .output()
        .unwrap();
    assert!(
        filters.status.success(),
        "浏览器主机筛选回归失败：\n{}\n{}",
        String::from_utf8_lossy(&filters.stdout),
        String::from_utf8_lossy(&filters.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&filters.stdout).unwrap();
    assert_eq!(report["cases"], 60);
}

#[test]
fn light_ui_refresh_health_and_catalog_preserve_interaction() {
    let interaction = Command::new("node")
        .arg("scripts/check-light-ui.mjs")
        .output()
        .unwrap();
    assert!(
        interaction.status.success(),
        "轻量界面交互回归失败：\n{}\n{}",
        String::from_utf8_lossy(&interaction.stdout),
        String::from_utf8_lossy(&interaction.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&interaction.stdout).unwrap();
    assert_eq!(report["cases"], 81);
    assert_eq!(report["fourColumnCases"], 21);
    assert_eq!(report["cleanup"]["groupEmpty"], true);
    assert_eq!(report["cleanup"]["profileRemoved"], true);
}

#[test]
fn light_ui_open_waits_for_request_and_response_completion() {
    let regression = Command::new("node")
        .arg("scripts/check-light-ui-open-wait.mjs")
        .output()
        .unwrap();
    assert!(
        regression.status.success(),
        "打开会话双阶段等待回归失败：\n{}\n{}",
        String::from_utf8_lossy(&regression.stdout),
        String::from_utf8_lossy(&regression.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&regression.stdout).unwrap();
    assert_eq!(report["passed"], true);
    assert_eq!(report["requestBlockedWait"], true);
    assert_eq!(report["responseBlockedWait"], true);
}

#[test]
fn transfer_feedback_waits_for_receipts_and_preserves_interaction() {
    let output = tempfile::tempdir().unwrap();
    let result = Command::new("node")
        .arg("scripts/check-transfer-feedback.mjs")
        .arg(output.path())
        .output()
        .unwrap();
    let diagnosis = std::fs::read_to_string(output.path().join("failure.json")).unwrap_or_default();
    assert!(
        result.status.success(),
        "等待反馈回归失败：\n{}\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr),
        diagnosis
    );
    #[derive(serde::Deserialize)]
    struct TransferReport {
        cases: Vec<String>,
    }
    let summary: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    let evidence = std::fs::read(output.path().join("transfer.json")).unwrap();
    let completed: TransferReport = serde_json::from_slice(&evidence).unwrap();
    assert!(!completed.cases.is_empty(), "等待反馈脚本没有执行场景");
    assert_eq!(summary["cases"], completed.cases.len());
    check_transfer_scenarios(&completed.cases);
}

fn check_transfer_scenarios(completed: &[String]) {
    for required in [
        "临时落位不修改已确认记录",
        "A迟到回执不改变当前B主机同UUID卡片",
        "刷新尚未返回也立即使用回执的原生位置",
        "晚到的旧快照不能回滚成功结果",
        "冲突恢复已知位置并留下核对说明",
        "丢回执加读取失败不伪造回滚",
        "恢复读回实际位置而不重发",
        "missing回执保持待核对且不伪报成功",
        "wrong-host回执保持待核对且不伪报成功",
        "wrong-id回执保持待核对且不伪报成功",
        "空原因失败后保留空草稿",
        "等待失败后的空草稿重开仍为空",
        "刷新前完整采用确认版本列及新输入提示",
        "未归列确认保留实际位置并显示可读完成提示",
        "只读卡片保持只读",
        "新输入冲突保留较新的状态",
        "重载读真实位置且不重放操作",
    ] {
        assert!(
            completed.iter().any(|name| name == required),
            "缺少已通过的场景「{required}」"
        );
    }
}
