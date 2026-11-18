# 一键归档结果

owner: codex
execution_mode: isolated_write
allowed_write_set: src/, tests/, index.html, assets/, scripts/, acceptance/bindings.json, Docs/, context/tasks/
base_commit: 5deef00dd55f4dace3b46b92bbf85570cd0eb446
worktree_path: /home/herman/.codex/worktrees/archive-batch-results/notion-program
coordinator: codex

页面逐条提交。看板保存不可变名单与每项结果，重开后从「归档结果」查看。核对通过同一主机与账号读取原生归档列表。原生结果未知时记录待核对，重复读取不会发送归档请求。

隔离测试覆盖丢失原生回执、重开后卡片已缺席、切换账号、停止尚未提交项、新输入拒绝及原生写次数。浏览器夹具覆盖回执丢失后的结果入口和只读核对；MCP 桥接夹具覆盖四个批次入口。

## 合并与发布

PR #27 的最终提交 `1f9a693238829b2cf3439c44a57d588cd68d2087` 通过独立复审和本地 1129 项验证。2026-11-18 23:26（北京时间）合入 `11e30abb1ccab139eed5315c9898464d8dd08aee`，冻结版本 v0.4.11 的源码为 `456ca9a28217b462f3b4e57929145e38e330afc3`。[PR #27](https://github.com/HYBB-rash/codex-session-board/pull/27)

同一候选的首次 macOS 验证中，已有远程连接隔离测试的整组前台操作耗时 1080.208375 毫秒，上限为 1000 毫秒。其中本地保存 59.44875 毫秒，归档 63.892291 毫秒。第二次 macOS 作业排队后取消。第三次完整复跑 Linux 1129 项、macOS 1118 项全部通过。连接测试继续使用原门槛，偶发超时原因待查。[首次运行](https://github.com/HYBB-rash/codex-session-board/actions/runs/44325301077/attempts/1) · [第三次运行](https://github.com/HYBB-rash/codex-session-board/actions/runs/44325301077/attempts/3)

Linux x86_64 和 macOS arm64 两个不可变候选包均通过隔离启动，并从候选渠道读回相同源提交和版本。日用客户端安装、重载和真实环境验收仍待安排。[发布工作流](https://github.com/HYBB-rash/codex-session-board/actions/runs/44335749123)
