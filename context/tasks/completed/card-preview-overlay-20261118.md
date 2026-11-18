---
owner: agent
execution_mode: isolated_write
allowed_write_set: "本任务记录、回复阅读面板布局和交互、对应测试"
base_commit: 96a82e5224a933a52415e3561a81a3d10ee39573
worktree_path: /home/herman/.codex/worktrees/card-preview-overlay/notion-program
coordinator: agent
---

# 恢复最近回复预览的蒙版弹出行为

右侧阅读栏浮在看板上方。点击灰色空白收起阅读栏和蒙版，保留筛选、列宽和滚动位置。
单击可见卡片可以切换，迟到的回复不会串到另一张卡或重开阅读栏。Esc 将焦点还给当前卡片。
首件只试了列内空白，正式回归增加灰色右沿、列标题、短窗滚动和长标题，
在备用网页与隔离 MCP 中发送真实指针事件。两个入口均通过，独立审阅通过。

## 合并与候选发布

2026 年 11 月 19 日 00:02（北京时间），[PR #28](https://github.com/HYBB-rash/codex-session-board/pull/28)
合入 `e66cb9a93f80d5e62ce73b9b702e9b2aa222b7c6`。PR 的最终提交 `5b7d8e8908c7fd73a4c34d878e5d4203409d6117`
通过 [PR 检查](https://github.com/HYBB-rash/codex-session-board/actions/runs/44338809369)。
同一冻结源码 `cefc8a611871cc0f0368718ee5001cbd999561a8` 在 [组合门禁](https://github.com/HYBB-rash/codex-session-board/actions/runs/44339366691)
通过 Linux 1129 项与 macOS 1118 项验证。

2026 年 11 月 19 日 00:12:59（北京时间），[v0.4.12 不可变候选发布](https://github.com/HYBB-rash/codex-session-board/releases/tag/v0.4.12)。
两个包均完成隔离启动和六个场景验证，从两个候选渠道读回同一冻结源码和包清单。

| 候选渠道 | 包 | SHA-256 |
| --- | --- | --- |
| `candidate/x86_64-unknown-linux-gnu` | `plugin-x86_64-unknown-linux-gnu.tar.gz` | `14363fbc0d0031a1e844de3a8e81b2d8e6528c7ab1b22efc32f8c60559418247` |
| `candidate/aarch64-apple-darwin` | `plugin-aarch64-apple-darwin.tar.gz` | `1afefb8ff8ab9e48233cb136dc24b2f2466d7001c6cf229f7d26c14a917bc243` |

[发布工作流](https://github.com/HYBB-rash/codex-session-board/actions/runs/44342596057)。
本机交付验证、独立复审、浏览器截图和包校验回执位于 `/home/herman/.local/share/card-preview-overlay-20261118/`。
真实环境验收和稳定版发布待安排。
