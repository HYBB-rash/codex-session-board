# PR #29 重载异常回执

2026-11-19 在独立作者工作树修复。反馈对应 `addf948`，见 PR #29 评论 `7002132469`。

用 `websocket-client==1.9.2` 在重载 RPC 的响应读取处分别注入连接关闭和超时。修复前两种异常都传播到更新进程。修复后两种异常都进入既有重载失败处理，保留安装确认、候选构建摘要及具体错误。worker 写入 `awaiting_desktop_reload` 并清理待处理回执。

隔离 MCP 进程重启测试分别读回两种失败回执。当前构建与回执匹配时，更新状态为 `active`，会话备注仍可读。

复现、定向回归与 Linux 提交门禁记录位于 `/home/herman/.local/state/codex-session-board/update-button-20261119/pr29-reload-repair/`。
