# 目标主机项目与会话目录

- owner: root
- execution_mode: isolated_write
- allowed_write_set: src/catalog*, src/app*, src/auto_archive.rs, src/observer.rs, src/store*, src/lib.rs, src/native_sections*, src/rpc*, src/remote*, index.html, tests/ui_layout.rs, tests/native_authority/archive.rs, scripts/test_remote_turn_proxy.py, scripts/remote_turn_proxy.py, scripts/bind_rust_tests.py, scripts/check-remote-catalog.mjs, examples/dev_fixture*, acceptance/bindings.json, README.md, Docs/REMOTE-CATALOG-DESIGN-20261114.md, context/results/remote-catalog*, context/tasks/active/remote-host-catalog-20261114.md, context/tasks/completed/remote-host-catalog-20261114.md, context/journal/2026-11-15-remote-host-catalog.md
- base_commit: d4b40bb3aae98569a310f1e412defbb1f20dddbb
- worktree_path: /Users/hongyong/Developer/codex-session-board-remote-catalog-impl-20261114
- coordinator: root
- status: in_progress

## 授权与目标

用户授权执行已经取证的目录设计并提交 PR。首次登记并连接主机之后，
会话看板读取目标主机全部当前项目、普通活动及归档历史和已发布持久引用，
无需操作机重建目标项目。此任务不包含部署、日用插件重载或新增项目写能力。
PR 创建后必须继续核对最终提交的 GitHub 检查，检查通过才算完成 PR 交付。

## 实施

1. 限权 CatalogReader，保持现有来源、SSH、转发与参数边界。
2. owner 投影、目标目录、迁移归属、持久引用、独立 presence 和续读缓存。
3. 主机项目与活动/归档筛选、缓存提示、能力限制及已有会话打开。
4. 隔离回归、自动验收、独立审查和 PR。

RPC worker 使用独立 remote-catalog-rpc-20261114 工作树；
legacy/membership worker 使用独立 remote-catalog-legacy-20261114 工作树。
root 独占本树整合、任务记录、索引与提交。不修改版本清单和共享 STATE。

## 验证

专项目录回归、断线草稿写入回归、Clippy 和隔离联调 10/10 已通过。
完整验收发现的原生 owner 校验和旧 fixture 未知方法处理问题已修复。
原生 owner 12 项、直接读取 8 项、原生接口 38 项及 Clippy 通过；
625 项原生测试和 145 项 Python 测试均有需求绑定。完整验收及修复提交的 PR 检查待重跑。
保持原 31 条需求、参考模型及锁；真实客户端缺证继续待验证。
