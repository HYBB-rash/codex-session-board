# 待归档按时间正序

owner: codex
execution_mode: serial_write
allowed_write_set: index.html, README.md, context/STATE.yaml, context/tasks/active/archive-oldest-first.md, context/tasks/completed/archive-oldest-first.md, context/journal/2026-11-06-archive-oldest-first.md
base_commit: 97bed312b24e3d2e9a2c9a3be3df7a232791c21f
worktree_path: /home/herman/文档/notion-program
coordinator: codex

用户要求待归档列按时间正序，最久远的卡片在最上面。
沿用卡片现有的会话更新时间 updatedAt，仅在待归档列排序，保留其他列的顺序。
通过实际 render 函数验证顺序、搜索过滤、重复刷新和空列。
完整验收缺少目标客户端证据时保持待验证，不提交、不更新版本、不部署。

本地实现与检查日期：2026-11-06。完整发布验收仍待验证。

- 实际执行 index.html 的 render 函数，验证最早在前、同时间稳定排序、其他列顺序、搜索、重复刷新、空列和原数据不变，全部通过。
- 完整验收报告：/tmp/session-board-archive-oldest-first-acceptance.json，228 个自动测试通过；缺少真实环境证据，退出 2，release_ready=false。
- git diff --check 通过。
- ./scripts/check 的注释、文档、钩子、unsafe 与版本检查通过；仅因本次改动未提交而退出 1，保留改动等待用户授权提交。
- sh scripts/verify_context.sh 通过。
