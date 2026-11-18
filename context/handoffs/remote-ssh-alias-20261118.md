# 远程新建 SSH 别名配置交接

任务：`context/tasks/active/remote-ssh-alias-20261118.md`
首件已获认可，授权写入与拒绝恢复已接入。

隔离浏览器覆盖已登记项目的原新建请求、全新连接的变更预览与拒绝后的页面重开和重新启用、旧连接引导至 Codex 原项目。Rust 回归核对已绑定别名时的导航回执，以及无别名时登记字节、远端请求及新建操作记录。

浏览器回执及截图：`/home/herman/.local/share/codex-remote-ssh-preview.yyL9EX/`。

开发范围和操作见 `Docs/REMOTE-SSH-SETUP.md`。

## 隔离验收

- 在原装 Codex Desktop `26.1111.21308`、回环 OpenSSH 和模拟模型服务中，从看板 HTTP 入口执行预览、拒绝、重启、重新启用和确认。预览与拒绝期间配置摘要一致。确认后产生一个 `Host` 段、一个 Codex 连接及一条目录登记。自动连接和同一授权重试均读回已登记，客户端只收到一次打开请求。首条输入前远端会话数为零。
- 原生界面发送测试消息后，从独立官方后台连接读回会话 `01a1edfe-2801-7750-9edc-17065947ad67`，`cwd=/tmp/csb/project`，`originator=Codex Desktop`。后台 `projectId` 起初为空。通过官方 `thread/metadata/update` 补登记后，独立连接读回同一会话及项目 `01a1edfd-f7eb-7d61-8853-a7a96a25d44a`。
- 原装客户端隔离证据保存在 `/tmp/csb-board-originator.vVI0D9a9/evidence/`。检查 `board-actions.json`、`readback-and-replay.json`、`project-verify.json` 和 `cleanup.json`。浏览器界面及桥接另有自动回归。
- 已登录账号场景待现场验证。看板「归入项目」入口已通过 Rust 与浏览器回归，原装客户端现场通过独立官方 API 完成补登记。全仓目标环境验收保持待验证。
