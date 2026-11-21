# PR #29 重载异常回执

2026-11-19 在独立作者工作树修复。反馈对应 `addf948`，见 PR #29 评论 `7002132469`。

用 `websocket-client==1.9.2` 在重载 RPC 的响应读取处分别注入连接关闭和超时。修复前两种异常都传播到更新进程。修复后两种异常都进入既有重载失败处理，保留安装确认、候选构建摘要及具体错误。worker 写入 `awaiting_desktop_reload` 并清理待处理回执。

隔离 MCP 进程重启测试分别读回两种失败回执。当前构建与回执匹配时，更新状态为 `active`，会话备注仍可读。

复现、定向回归与 Linux 提交门禁记录位于 `/home/herman/.local/state/codex-session-board/update-button-20261119/pr29-reload-repair/`。

组合门禁的 macOS 运行 `44499016726` 中，两条重载测试在检查私有快照归属时退出。测试夹具沿用 `/tmp` 的别名路径，worker 解析脚本路径后得到真实父目录。Linux 上用隔离临时目录中的符号链接复现两条相同的 `ValueError`。夹具解析根目录后，两条测试均通过，生产归属检查仍在原位置执行。复现与复验记录位于 `/home/herman/.local/state/codex-session-board/update-button-20261119/pr29-macos-fixture-repair/`。

2026-11-20 检查组合候选 `5a6113bdfcd4e2c0ba9f71eddf111d91016006da` 的 macOS 回执。两次失败都发生在 MCP iframe 中，恢复宽屏后点击阅读栏右缘，预览没有收起。Mac 本机隔离环境使用与失败镜像同版的 Chrome 156.0.8078.80，原脚本连续八次通过，带点击诊断的 24 次运行也通过。GitHub runner 丢失该次关闭动作的具体原因待确认。

迟到回复用例改为点击明确的列标题，并先验证落点确实命中该标题。继续通过真实鼠标点击检查收起，释放延迟回复后检查预览仍收起且正文为空。其余窄屏、阅读栏边缘和滚动位置断言继续运行。Linux 和 Mac 的定向日志及冻结检查器报告位于 `/home/herman/.local/state/codex-session-board/update-button-20261119/pr29-reply-preview-repair/`。
