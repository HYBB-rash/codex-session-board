---
owner: agent
execution_mode: isolated_write
allowed_write_set: "本任务记录、卡片及阅读面板、只读回复接口、验收契约与对应测试；原型位于本会话的独立预览目录"
base_commit: 464221b65b118b08506565297e99b576e0bd8d44
worktree_path: /home/herman/.codex/worktrees/card-preview-redesign/notion-program
coordinator: agent
status: completed
---

# 会话看板卡片预览改版

单击卡片或标题阅读最近一条完整助手回复。阅读面板保留进入原会话的入口。
用户定稿后验证官方客户端的渲染能力和历史读回，再实现并更新 NAV-001。
终点为 PR 合并及经验证的同源 Linux/macOS 不可变候选包发布。

用户已批准右侧阅读栏。历史读回、Markdown 渲染和两条浏览器入口的验证记录见
`context/handoffs/card-preview-redesign-20261118.md`。浏览器回归覆盖快速切卡、收起后的迟到结果、
阅读中完成新回复、空答复与读取失败、拖动、键盘焦点、Markdown 安全以及按 ID 进入原会话。
`NAV-001` 已随验收契约 v3 更新。

## 合并与发布

- PR 最终头 `20261432370a1f05c0f207aff484a13b53dad6b2` 通过独立复审和 PR 检查。
  合并提交为 `2bc87063af7591ffe66ef1f00a9aa092213cb06a`，冻结源码为
  `0c184e0bbea868a9dd28e4eccfbea2b6c87ef91f`。[PR #26](https://github.com/HYBB-rash/codex-session-board/pull/26) · [PR 检查](https://github.com/HYBB-rash/codex-session-board/actions/runs/44302401041)
- 最终组合检查通过 Linux 1127 项、macOS 1116 项。前两次 macOS 分别出现前台耗时超限
  与远程扫描超时，第三次等待 runner 后被取消。隔离 Mac 对照中主线与同一候选均通过原生权限组的
  39 项用例，前两次超时原因仍未确定。第四次使用相同源码与断言通过。[组合检查](https://github.com/HYBB-rash/codex-session-board/actions/runs/44302636365/attempts/4)
- 双平台包均完成独立启动与六项场景检查，其中包括一次成功启动、五项预期故障。
  下载包、校验文件和验证报告均核对 GitHub 摘要，两个 `candidate/` 渠道的清单均与包内清单一致。
  v0.4.10 于 2026-11-18 22:14:48（北京时间）发布为不可变候选。[发布](https://github.com/HYBB-rash/codex-session-board/releases/tag/v0.4.10) · [发布验证](https://github.com/HYBB-rash/codex-session-board/actions/runs/44319237618)

| 平台包 | SHA256 | 候选渠道提交 |
| --- | --- | --- |
| `plugin-x86_64-unknown-linux-gnu.tar.gz` | `5464b3f8068370e9be6dbdeb06f4ee153f894f0edf8962624b55c5f88f2cec2d` | `9f4e6fc9e12cddbb120237c0ce4654f595c24abe` |
| `plugin-aarch64-apple-darwin.tar.gz` | `416e1cf616ade4803d919373e370d94333d39b5166dcee9a2d67e14dbce15d33` | `4967a7e645e5a5ad46e1aa5a80641fe81ed79c0b` |

本机交付证据保存在 `~/.local/share/card-preview-redesign-20261118/`，
其中 `recombined-browser/` 保存网页及 MCP 截图，`release-v0.4.10/verified-release.json`
保存包、启动、场景报告及渠道读回摘要。稳定版与真实环境验收另行安排。
