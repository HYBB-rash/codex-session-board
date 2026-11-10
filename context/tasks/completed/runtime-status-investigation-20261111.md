# 核查运行中的会话仍显示待审阅

owner: codex
execution_mode: coordinated_read
allowed_write_set: context/, Cargo.toml, Cargo.lock
base_commit: 280b20b874e9491f638dfdf59285091a26ef7930
worktree_path: /home/herman/文档/notion-program
coordinator: codex

用户观察到正在执行的会话仍位于待审阅，要求确认状态跟踪问题是否存在。
主代理只读检查正式进程、健康信息和会话运行证据；两个只读子代理分别检查原生运行阶段与拖动保护、现有回归覆盖。
不修改正式配置、生产库和用户会话位置，不发送消息制造执行。
确认具体反例、判断影响范围和可能原因后，保存可复查的诊断结论。

## 结论与现场证据

2026-11-11 01:43–01:47，Asia/Shanghai，已确认本地真实反例。
正式 HTTP 服务为 v0.2.73，PID 3296170，构建 SHA256
`0b0a7137453b66de1c49940d82236ef9a3d8fdfaaec8b11db22718e50044a570`。
仓库 v0.2.74 与该版的运行跟踪和移列源码相同；差异只有版本与部署记录。

只读连接当前桌面 app-server，执行 initialize 和 thread/list，读回完整一页：

| 会话 | 桌面实时状态 | 原生分区与看板列 |
| --- | --- | --- |
| `01a1b4de-ab7c-7841-bd77-3c0384dde49a`，调查远程会话重命名功能未实现原因 | active，activeFlags 为空 | 待审阅 |
| `01a1c68c-4987-78c3-9869-80ab3b2566cc`，本次诊断会话 | active，activeFlags 为空 | 未分区，看板为待判断 |

通过正式 /api/sessions 和 SQLite mode=ro 交叉核对：
两个会话均已有新轮次的 `runtime_pending_key`，目标都是 `in_progress`。
第一个会话原有 `runtime_key` 属于上一轮 review，`selected_state` 为 null，
`native_choice_at` 为 null。当前运行阶段已经通过保护判断并进入移列队列。
历史读取和实时读取会交替更新 `turn_json`，故不使用单次历史 interrupted
判定当前执行是否停止，以同一桌面 authority 的实时 thread/list 为准。

健康信息连续读回 `runtimeVerified=true`、`nativeVerified=true`，
但 `nativeStageError` 为两条“客户端分区连接暂不可用，等待客户端恢复后自动重试”。
运行状态读取和原生位置读取成功，不表示移列写入连接成功。

配置中的旧路径 `/tmp/codex-app-tools-1000/fb575152-130b-4151-8d2b-f4b1ee166455.sock`
不存在。`ss -xlpn` 仍显示桌面进程监听另一个端点
`/tmp/codex-app-tools-1000/658f40c4-9293-42ca-9841-6860633b05c4.sock`，
但该路径也不存在，实际 connect 返回 ENOENT。
`/tmp/codex-app-tools-1000` 和 `/tmp/codex-app-tools` 两个发现目录均不存在。
桌面进程、看板进程和诊断进程的挂载命名空间相同，排除不同挂载视图。
尚未查明路径消失的时间、原因和执行者。

## 代码对应与边界

`src/store/runtime_native.rs` 接受新阶段后才写入 pending。
`src/app/native.rs:64` 建立 SidebarConnection 失败，尚未进入 move_checked，
也未执行自动状态提交。`src/rpc/sidebar.rs:108` 的无可连接端点错误与现场一致。
`src/observer.rs:144` 单独记录 nativeStageError，运行扫描仍可标记已连接。
`index.html:628` 与 `index.html:632` 会分别显示连接成功和自动更新待重试。

本次反例的直接原因是分区写入连接不可达，不能归因于人工拖动保护。
失败 pending 已持久保存，恢复唯一可用端点后仍需实测补偿与新阶段流转。
现有恢复测试覆盖旧路径失效后发现新端点，未完整覆盖 listener 仍存活、
两个目录和 socket 路径都消失后再重建的端到端过程。

专项隔离验证通过：

- `cargo test --locked --offline --test native_authority runtime::`，4 项。
- `cargo test --locked --offline --lib rpc::sidebar::tests::`，9 项。

未修改业务代码、生产配置、数据库和用户会话位置，未重启桌面或看板。
本次只完成问题确认，未修复运行环境，不宣称完整发布验收或 release_ready。
