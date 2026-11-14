use serde_json::Value;

pub const STATES: [(&str, &str); 6] = [
    ("unknown", "待判断"),
    ("in_progress", "进行中"),
    ("review", "待审阅"),
    ("blocked", "已阻塞"),
    ("to_archive", "待归档"),
    ("completed", "已完成"),
];

pub fn board_state(state: &str) -> &str {
    if state == "completed" {
        "to_archive"
    } else {
        state
    }
}

pub fn board_states() -> impl Iterator<Item = (&'static str, &'static str)> {
    states_for(false)
}

pub fn states_for(legacy: bool) -> impl Iterator<Item = (&'static str, &'static str)> {
    STATES
        .into_iter()
        .filter(move |(state, _)| legacy || *state != "completed")
}

pub fn runtime_state(status: &Value, turn: &Value) -> Option<(&'static str, &'static str)> {
    match status["type"].as_str() {
        Some("active") if turn["status"] == "inProgress" => {
            let flags = status["activeFlags"].as_array();
            if flags.is_some_and(|values| values.iter().any(|flag| flag == "waitingOnApproval")) {
                Some(("blocked", "等待你批准后继续执行。"))
            } else if flags
                .is_some_and(|values| values.iter().any(|flag| flag == "waitingOnUserInput"))
            {
                Some(("blocked", "执行暂停，等待你回答交互问题。"))
            } else {
                Some(("in_progress", "AI 正在执行本轮任务。"))
            }
        }
        Some("active") => None,
        Some("systemError") => Some(("blocked", "执行遇到系统异常，需要处理后继续。")),
        Some("idle") => terminal_state(turn),
        Some("notLoaded") if turn["status"] == "completed"
            || turn["completedAt"].as_f64().is_some_and(|time| time.is_finite() && time > 0.0) => terminal_state(turn),
        _ => None,
    }
}

fn terminal_state(turn: &Value) -> Option<(&'static str, &'static str)> {
    match turn["status"].as_str() {
        Some("failed") => Some(("blocked", "本轮请求失败，需要处理后重试。")),
        Some("interrupted") => Some(("blocked", "本轮执行已中断，需要检查后继续。")),
        Some("completed") => Some(("review", "本轮执行已结束，等待你审阅结果。")),
        _ => None,
    }
}

pub fn recommend(turn: &Value) -> (&'static str, &'static str) {
    match turn["status"].as_str() {
        Some("completed") if turn["hasFinal"] == true => (
            "review",
            "最后一轮已结束并有最终答复，请检查是否需要验收或继续。",
        ),
        Some("failed" | "interrupted") => (
            "unknown",
            "历史记录显示本轮失败或未确认结束，不能据此断定任务已阻塞。",
        ),
        _ => ("unknown", "尚无足够证据判断工作状态。"),
    }
}

pub fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |duration| duration.as_secs_f64())
}
