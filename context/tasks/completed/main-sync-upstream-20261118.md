# 本机 main 对齐上游 0.4.10

本机 main 与 `origin/main` 分别有 32 和 151 条提交。`git pull` 后的合并有 17 个文件冲突，
冲突行数 257，超过合并记录 13 文件、400 行的门限，不能作为一次合并记录落地。

逐条核对 32 条独有提交：26 条 patch 与上游等价，其余提交的内容也已在上游实现
（官方 SSH 别名导航、项目身份校验、拖拽反馈动画及回归均见上游同类提交）。本机 main 只多出
4 条记录提交：目标跨轮研究、归档丢回执事件、归档状态调研、前台提交设计。
冲突文件的 HEAD 侧版本与上游相比都是更早的实现，例如 `src/native_new.rs` 只有
`remote_url`，上游另有 `registered_alias`、`navigation` 与 `ApprovedNavigation`；
`index.html` 本机缺 `canOpen`、`ssh-setup` 和 `native-consent`。

因此把 main 对齐 `96a82e5`（0.4.10），再重放 `9c30d9f`、`8ca77b1`、`90c4c97`、`a7e4c55`
四条记录提交，只人工合并 `context/STATE.yaml`。合并前的头保存在
`backup/main-pre-merge-20261118`（`a7e4c55`），可随时取回原合并形态。结果树与逐文件取上游、
STATE 取并集的合并相同，代码、脚本、页面与打包清单全部来自上游。

`scripts/build-app-bridge.mjs --check` 的桥接产物与源码一致，`cargo check --all-targets`
通过。

owner: codex
execution_mode: serial_write
allowed_write_set: 本任务卡、项目上下文收尾与本地 Git 历史
base_commit: 96a82e5
worktree_path: /home/herman/文档/notion-program
coordinator: 当前会话
status: completed

## 采用与保留

- 采用上游实现：四栏看板、SSH 新建设置、答复预览、卡片预览、收紧的写入授权与全部验证工具。
- 保留本机记录：四条研究/设计任务卡、对应 journal 与 `STATE.yaml` 的待决事项。
- 未推送：本机 main 未推送任何提交，日用插件未安装或重载，验收证据不因本次同步变化。
