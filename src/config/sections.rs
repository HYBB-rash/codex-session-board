use super::Config;
use crate::model;
use anyhow::{Context, Result, ensure};
use std::collections::HashSet;

impl Config {
    pub fn section_bindings(&self) -> impl Iterator<Item = (&'static str, &'static str, &str)> {
        model::board_states().filter_map(|(state, label)| {
            self.status_sections
                .get(state)
                .map(|identifier| (state, label, identifier.as_str()))
        })
    }

    pub fn section_target(&self, state: &str) -> Result<&str> {
        self.section_bindings()
            .find(|item| item.0 == model::board_state(state))
            .map(|item| item.2)
            .context("目标原生分区未配置")
    }

    pub(super) fn validate_sections(&self) -> Result<()> {
        ensure!(self.sidebar_state_file.is_absolute(), "sidebar_state_file 必须为绝对路径");
        ensure!(uuid::Uuid::parse_str(&self.sidebar_caller).is_ok(), "sidebar_caller 必须为会话 ID");
        ensure!(self.sidebar_socket.is_absolute(), "sidebar_socket 必须为绝对路径");
        ensure!(
            !self.status_sections.contains_key("completed"),
            "原生配置含已退役的已完成分区，请先迁移配置并移除该映射"
        );
        ensure!(
            self.status_sections.keys().all(|state| model::board_states().any(|item| item.0 == state)),
            "包含未知状态分区"
        );
        let mut identifiers = HashSet::new();
        for (state, label) in model::board_states() {
            let identifier = self.status_sections.get(state)
                .with_context(|| format!("缺少状态分区：{label}"))?;
            ensure!(uuid::Uuid::parse_str(identifier).is_ok(), "状态分区 ID 无效：{label}");
            ensure!(identifiers.insert(identifier), "状态分区不得重复：{label}");
        }
        Ok(())
    }
}
