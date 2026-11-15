# 轻量 UI 实施

- owner: codex
- execution_mode: isolated_write
- allowed_write_set: index.html; assets/*; src/mcp.rs; src/http.rs; tests/ui_layout.rs; tests/board_io.rs; scripts/*bridge*.mjs; scripts/browser-client.mjs; scripts/check-ui-layout.mjs; scripts/check-light-ui.mjs; scripts/check-remote-catalog.mjs; acceptance/bindings.json; Docs/LIGHT-UI.md; context/tasks/active/light-ui-implementation-20261115.md; context/results/light-ui-implementation-20261115.md
- base_commit: d4b40bb3aae98569a310f1e412defbb1f20dddbb
- worktree_path: /Users/hongyong/Developer/codex-session-board-light-ui-implementation-20261115
- coordinator: 后续集成协调者
- status: implemented_awaiting_integration

用户于 2026-11-15 授权按已调查的轻量方案实施。
方案来源为 light-ui-plan-20261115 的 3d7308f；原型为 6a53ff7。
共享根工作树在建树前干净。本任务保持 0.3.85，不改共享 STATE，不推送或部署。
使用 coding-taste 和 unslop 技能，小步提交，并执行隔离 fixture 与项目回归。

## 顺序

1. 恢复可读 SDK 入口、依赖锁和可复现构建，接入宿主主题。
2. 移植轻量视觉，完成短窗口、全文备注、调整入口和刷新焦点保护。
3. 对照远程目录已提交字段，保留历史、缓存、只读和能力限制，统一健康判断。
4. 独立浏览器、WebKit、真实 Rust fixture 与自动回归，交付可审阅分支。

远程目录工作树仍有未提交开发。兼容参考固定为 9160e4b，不能视为最终头。
本任务基于 main 独立开发，不复制目录后端；后续整合须冻结并复验最终目录头。
原 31 条需求、参考模型和锁保持；独立探针不能签收日用原生容器。

## 实施进展

桥接源码、依赖锁、宿主主题、轻量布局、短窗口、全文备注、调整入口与焦点保护已实现。
远程 UI 参考 1fe6caf、2b636d7，冲突按标题优先和历史权限限制合并；
选择主机保持纯筛选，未引入参考分支的隐式刷新请求。
健康判断额外要求项目资料就绪，保留缓存、错误、未完成与刷新等待状态。

独立浏览器已通过桥接 31、轻量交互与健康 80、目录兼容 44、主机筛选 53、
布局 48、保存等待 42 项检查。3c9b578 完整自动门禁 714 项通过，Clippy 通过。
系统 WebKit 浅深色探针、真实 Rust fixture 6 项交互与 10 项可选后台隔离检查通过。
独立浏览器和 fixture 已清理，最终停止回执 cleanup_confirmed=true。
1500 张卡片不变时全部保留，八次 render 为 4–21ms；首次构建约 138ms，非日用容器计时。
测试发现并修正来源占位未纳入列签名，以及浏览器主进程退出后子进程回收遗漏。
全量回归曾出现 3 项后端失败，单独复跑及随后整轮复跑通过，原失败记录保留；
不能据此认定启动与性能失败的根因已消除。本轮没有修改对应后端代码或断言。
代码验收头为 3c9b578，完整证据与后续组合边界见
context/results/light-ui-implementation-20261115.md。
后续协调者冻结目录最终头、执行组合门禁并归档本任务；日用原生容器验收仍待安排。

用户随后授权提交 PR 并确保检查通过。PR 为
https://github.com/HYBB-rash/codex-session-board/pull/12。
首次 Linux CI 暴露占位滚动条挤占 280px 外框内空间的问题，本机强制 16px 滚动条
复现可用区仅 264px。改为内容层约束，补充原生、16px 与 32px 滚动条回归；
轻量检查由 58 增为 80 项，四项 Rust 浏览器绑定全部通过，保留原有高度断言。
系统 WebKit 补验发现 min-content 仍遗漏滚动条空间，改用自动 flex 基准且禁止压缩；
320×256、16px 滚动条、深浅深主题实验的可用高度为 286px。
后续 PR 检查必须核对修复后的最终头，不能用 3c9b578 的旧报告签收新实现。
