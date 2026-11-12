# 主机筛选与异常提示

- owner: codex
- execution_mode: isolated_write
- allowed_write_set: index.html; scripts/check-host-filter.mjs; scripts/check-ui-layout.mjs; tests/ui_layout.rs; Docs/PROJECT-FILTERS.md; context/tasks/active/host-project-filter-20261113.md; context/tasks/completed/host-project-filter-20261113.md; context/handoffs/host-project-filter-20261113.md; context/journal/2026-11-13-host-project-filter.md; context/STATE.yaml; Cargo.toml; Cargo.lock; plugin.json; .codex-plugin/plugin.json
- base_commit: c15946e647b222029ee5938ec2ebdde740833155
- worktree_path: /Users/hongyong/Developer/codex-session-board-host-filter
- coordinator: codex
- status: awaiting_integration_and_target_validation

## 已授权方案

主机选择始终可见，具体主机下显示项目或搜索。切换主机重置全部项目，搜索保留主机范围。
新建固定当前主机，远程未选具体项目时先选择项目；批量归档只处理筛选结果。
离线和没有项目的主机仍可查看缓存。项目读取失败保留快照，确认移除时不扩大主机范围。
顶部先说明影响和处理办法，再展开原始诊断。后台连接、扫描及定时归档保持原逻辑。

## 验证与边界

使用独立开发 fixture 和独立浏览器，不安装、重载或重启日用插件。
覆盖同名项目、空项目、离线缓存、读取失败、目标固定、归档范围和窄窗口。
执行项目检查和完整验收入口。真实 Codex 证据缺失时继续标记待验证。

## 实现结果

候选实现通过 49 项交互、48 组布局及 6 项隔离原生服务检查；完整验收 629 项自动检查通过，14 项真实环境证据待补，release_ready=false。接手见 `context/handoffs/host-project-filter-20261113.md`，尚未合入主工作区或部署。
