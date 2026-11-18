# 2026 年 11 月 18 日一键归档故障调查

这批 **6 个会话都已在 Codex 原生端归档**。界面的「5 条成功」对应 5 个成功回执，逐条核验均真实归档。第 4 条「升级 Codex 会话看板插件」在看板的 HTTP 转发报错前就开始了原生归档，约两秒后完成。看板没有补齐这条的本地归档标记。约九分钟后查询实际看板接口，6 张卡片均已从列表消失。

**建议修复看板的归档结果核对和提示。** 环回连接瞬时中断属于应处理的故障。只要如实标明「结果待确认」并可安全核对，一次回执丢失可以接受。本次用户看到「未完成，已保留卡片」，稍后却找不到卡片，也没有收到归档确认，因此处理不合格。HTTP 异常链已被压缩，现有证据无法定位哪一端关闭了连接。这个未知影响传输层是否需要修复，不影响先修结果核对的决定。

owner: codex
execution_mode: serial_write
allowed_write_set: 本任务卡、项目上下文收尾、源码外证据与隔离复现
base_commit: 9c30d9f71e485c4e8dc334b609589f16e14d7cc1
worktree_path: /home/herman/文档/notion-program
coordinator: 当前会话
status: completed

## 事件与影响

时间均为北京时间。桌面内嵌看板在 19:05:51.628 至 19:05:58.687 依次调用 6 次 `board_archive`。下表按桌面调用的时序排列。用每次调用窗口、原生 `thread/archive` 日志、源会话 UUID、原生库和看板库交叉核对。日志没有跨层共用的请求 ID，第 4 条的时序另见下节。

| 序号 | 会话 | 调用开始 | 原生归档时刻¹ | 看板回执 | 原生库 / 看板库归档位² |
| --- | --- | --- | --- | --- | --- |
| 1 | 编写 Codex 自动推进调研委派书 `01a1edc8-8501` | 19:05:51.628 | 19:05:51.721 | 成功 | 1 / 1 |
| 2 | 验证 Codex 会话自动推进方案 `01a1edd1-a333` | 19:05:51.793 | 19:05:53.891 | 成功 | 1 / 1 |
| 3 | 验证看板最终答复预览可行性 `01a1ed9e-d054` | 19:05:53.962 | 19:05:54.125 | 成功 | 1 / 1 |
| 4 | 升级 Codex 会话看板插件 `01a1ee42-c87c` | 19:05:54.299 | 19:05:56.404 | 19:05:54.411 转发报错 | **1 / 0** |
| 5 | 用 Delegation Brief 帮我交代这个任务 `01a1ee4f-73f2` | 19:05:54.415 | 19:05:56.412 | 成功 | 1 / 1 |
| 6 | 调查待判断状态的来历与移除影响 | 19:05:56.540 | 19:05:58.659 | 成功 | 1 / 1 |

¹ 原生库 `archived_at` 精确到秒。表内毫秒取对应 `archived_sessions` 文件的 `ctime`，并与原生归档处理日志对照。第 3、6 条的原生归档分别带上 3、6 条子会话。表格仅列顶层卡片。

² 19:13:31 通过 SQLite 只读连接备份后核对。六条原生记录的归档文件均存在。第 4 条本地行仍是 `to_archive`，版本 4，`archived=0`。标题中的 `[待归档]` 不是归档标志，五条已确认归档会话也保留这个标题前缀。

`archiveSessions` 对每项串行等待回执。它只在收到成功回执时加一并删除页面内的那张卡片，随后用 `load()` 替换整个快照。它在异常分支先保留第 4 张卡片，但这只能解释批次结束、刷新之前的前端数据。没有当时的 DOM 或截图可核对用户看见了多久。19:15:20 对 `18767` 的只读 `GET /api/sessions` 返回 `nativeVerified=true`、`scanComplete=true`，六个 UUID 全部缺席。这与原生活动列表过滤一致。第 4 项在后台已归档，界面上也没有残留可操作卡片，残留在本地库和批次提示。

## 调用链与失败边界

事发看板为 `0.4.7`，产物记录源码 `464221b65b118b08506565297e99b576e0bd8d44`。运行二进制与已安装包的 SHA-256 均为 `db0aa1f8dc43bd64be78f9a161a5775f3e8ddcfc1d5724fdf2dfd4f8f135a1e3`。原生事发日志也记录 `app_server.client_version="0.4.7"`。取证时 `18767` 的监听者是看板 `mcp` PID `2085616`，18:46:40 启动。其上级原生后台 PID `774622` 与事发原生日志及 `/api/info.authority` 一致。Fedora 桌面包为 `codex-desktop-2026.11.17.194805-1.fc44.x86_64`，库中原会话的 CLI 版本为 `0.169.0-alpha.11.1`。本地 `HEAD` 的版本是 `0.3.97`，以下源码行号均取发布提交。

内嵌页面以 `board_archive` 发起 MCP 工具调用。接受工具的 MCP 处理走 `RuntimeHandle` 的 `Shared` 分支，校验共享服务，读取 `/api/info` 的令牌，再由 `reqwest` 发 `POST /api/archive`。接收端的 HTTP worker 校验请求并调用 `archive::request`。本机原生分区模式下，归档先校验本地版本和最新原生输入，经同一桌面的 Unix socket 发 `thread/archive`，然后提交看板库标记。出现上述 URL 文案的路径是 `POST … .send()` 返回错误，MCP 把错误压成 `error.to_string()`。同一工具的 JSON-RPC 信封仍成功返回，`errorCode=null` 与工具结果 `isError` 可同时成立。

事发关键日志按时序摘录。完整行和请求 ID 见证据目录。

```text
19:05:54.299125   Calling app tool … server=session_board … tool=board_archive
19:05:54.395642767 Submission thread_id=01a1ee42-c87c-7d60-bf9d-69df5bca8ff4 … operation="shutdown" … op=Shutdown
19:05:54.410433   response_routed … durationMs=110 errorCode=null … method=mcpServer/tool/call
19:05:54.411235   Widget tool failed error="error sending request for url (http://127.0.0.1:18767/api/archive)" … tool=board_archive
19:05:54.415116   Calling app tool … tool=board_archive
19:05:56.397026846 … Shutting down Codex instance
19:05:56.400251996 app_server.request{… rpc.method="thread/archive" rpc.transport="unix_socket" rpc.request_id=6 app_server.connection_id=10231 …}: clearing thread listener … thread_id=01a1ee42-c87c-7d60-bf9d-69df5bca8ff4 … had_active_turn=false
19:05:56.410764117 … rpc.method="thread/archive" … app_server.connection_id=10235 … thread_id=01a1ee4f-73f2-7da1-99cc-5588688c00b0 …
19:05:56.443481   response_routed … durationMs=2028 errorCode=null … method=mcpServer/tool/call
```

- **已确认执行。** 第 4 条的同一 UUID 在第 4 次调用区间已提交 shutdown，随后这个 shutdown 的同一 submission ID 执行完毕，原生 `thread/archive` 完成并落库、搬移文件。第 5 条由另一个原生连接处理。结合顺序、同一窗口的唯一错误和无自动归档配置，这是第 4 次 POST 已到达并触发归档、工具没有收到成功结果的证据。当前日志缺少 HTTP ingress 正文 ID，跨层关联依据是这些时间、UUID 和执行顺序。
- **最可能的故障区间。** POST 发出并开始处理后，转发端在获得可用 HTTP 响应前返回请求错误。MCP 往返只用了约 110 ms。取证配置中原生 RPC 等待为 20 秒，已配置侧栏 socket，转发超时按代码为 `5 × 20 + 10 = 110` 秒。即使未配置侧栏 socket，最低也是 8 秒。前后调用均工作，另有三次成功调用耗时约两秒。现有证据排除了正常请求超时及 CSRF、版本、待归档资格拒绝。已经开始处理的 POST 排除了建立连接前拒连。监听进程自事前持续存在，前后调用正常，缺乏服务重启或整体停摆证据。第 4 条的两秒收尾延长了归档执行，浏览器资源释放发生在报错之后。
- **底层原因仍未知。** 保留下来的 `reqwest` 文案只有请求类别及 URL。没有该调用的 `Error::source()` 链、HTTP 工作线程的接收和响应写入日志，或同时段环回 TCP 记录。无法在提前 EOF、连接被重置或其他发送/接收故障之间定案，也无法确认第 4 条的原生成功回执是否经 socket 送达看板。即使 HTTP 客户端收不到响应，HTTP worker 仍会继续执行，本地归档事务须单独核对。本地 `0` 与原生回执未到、确认事务失败或后续本地状态被恢复相容。缺少该 UUID 的本地事务轨迹，无法细分。现场原生 `archived=1` 且有文件，足以确定本次用户操作的最终结果。

## 状态、恢复与决策

`Docs/ARCHIVING.md:20` 规定重试先完整读原生归档列表，已归档就确认而不重写。第 29–44 行规定失败或丢回执时保留卡片和备注，允许用户再次核对。`Docs/NATIVE-STATE.md:121` 又规定原生只出现在归档列表时，普通同步保持本地行原样。两个规则叠加后，完整扫描可隐藏卡片，却留下归档标志未确认和原错误提示，用户从卡片重试的入口也消失了。

本次原生侧已归档，无需再归档。若同类错误后卡片仍在，应先在原生归档列表核对该会话。代码上，同一看板内对同一 UUID 串行，当前版本条件满足且归档列表读回完整时，重复同一请求只补本地标记，不再发 `thread/archive`。原生查不到已归档且列表完整时，才重新核对分区、输入和空闲状态后执行。版本变了、列表读不全或原生状态不明时会拒绝。这个保护依赖读回和同一运行时的锁。原生接口没有跨客户端原子条件，不能把它当作任意断线和并发下可以盲重放的保证。

建议把最小改动放在 **看板归档服务与 MCP 转发的结果边界**。

1. 将执行前明确拒绝、确认已归档和写入结果待确认作为不同结果返回。对不明确的归档写入，按主机和 UUID 做一次有界的原生只读核对。原生已归档才补本地标记，并沿用现有版本与备注保护。原生在处理中、无法读回或证据仍不全时，保持待确认。HTTP 转发丢结果时同样从原生已归档证据核对，不能用卡片缺席替代。
2. 批量汇总按核对后的结果更新。原生状态未确认时给用户保留「核对结果」入口，该入口只查询，不重发归档。如果核对已完成，显示已归档，允许列表正常消失。验收至少覆盖请求未送达、收到后仍在执行、原生成功但回执丢失和本地标记提交失败四种情况，并断言成功项不重发、备注保留、后续条目继续。
3. 在转发端记录异常链和阶段，给每项加跨层关联 ID。HTTP 接收端记录请求进入、原生请求开始/结束、本地确认结果和响应写入结果。保留阶段、耗时和必要标识，避开令牌与会话正文。`tiny_http 0.12.0` 的 `respond` 还会吞掉若干对端关闭错误，单记这一返回值无法证明对端收到了响应。如再发生，用同一 ID 的日志判断断点。若仍需要区分谁先断开，仅在隔离复现采集环回连接的 TCP 元数据，与 HTTP 两端事件对齐。

本地归档与状态不一致可独立修复。若暂缓，下一次回执丢失仍可能出现「操作其实已完成，页面长期报未完成且卡片消失」，用户只能离开看板去原生端查证。现在没有依据先调大超时、改 Codex 客户端或替换 HTTP 库。

## 证据索引与复验

源码外的证据目录为 `/home/herman/.local/share/codex-session-board-investigations/20261118-archive/`。文件完整性可用 `sha256sum -c SHA256SUMS` 核验。

| 证据 | 查询与结果 |
| --- | --- |
| `desktop-journal.log`、`native-log.tsv` | 先取得 11 月 18 日用户 journal，再从 Codex `logs_2.sqlite` 的只读快照按事发时间、`thread/archive` 和第 4 项 UUID 提取。原生日志有纳秒时间、进程与连接 ID。 |
| `state-join.json` | `state_5.sqlite.threads` 与实际服务 FD 指向的 `status.sqlite3.sessions` 按 UUID 左连。文件附查询、时间参数和逐条结果。核对原生归档位、看板归档位、父子关系及原生文件存在性。 |
| `release.json`、`runtime-observation.txt`、`runtime-info.json`、`live-card-projection.json` | `ss`、进程启动时间、`/proc/2085616/exe` 哈希、只读 `GET /api/info` 和 `/api/sessions`。核对 PID、构建、原生后台与六个 ID 的当前卡片投影。 |
| `isolated-forward.jsonl`、`repro-forward.py` | 用发布包的实际 MCP 二进制和临时目录、随机回环端口。测试 HTTP 端接受并处理模拟 POST 后强制断开，MCP 返回同形文案，下一条成功。对照场景中的 HTTP 400 则保留业务拒绝文字。 |
| `isolated-http-chain.txt`、`probe-http-chain.rs` | 用发布锁定的 `reqwest 0.12.28` 对随机回环端口复验。请求被接受后提前关连接给出 `SendRequest → connection closed before message completed`。拒连给出 `Connect → tcp connect error → Connection refused (os error 111)`。最外层文字同形，因此不能用复现的异常链补写现场。 |
| `isolated-native-ack.log`、`native-probe.fn.rs` | 发布源码的临时副本中复用 `tests/native_authority/archive.rs` 的 fixture，仅增加隔离探针。原生置已归档后丢 RPC 回执，初始卡片仍在。完整活动扫描后卡片隐藏，本地仍未标归档；用原版本隔离重试只读回并补确认，`thread/archive` 总计 1 次。这个实验解释可产生相同状态的路径。 |
| `isolated-archive-flow.log`、`isolated-batch.txt`、`repro-batch.mjs` | 在发布源码副本执行 `cargo test --offline --locked --test archive_flow`，11 项通过。用 Node 执行该版本的实际 `archiveSessions` 函数，第四项抛错后继续，显示「5 成功、1 未完成」；一次新快照清空卡片，消息仍保留。 |

源码复核用 `git show 464221b:path | nl -ba`，将 `path` 替换成下列文件。

- `index.html:292` 选择内嵌调用或直接 HTTP，`:635` 逐项统计，`:1233` 刷新数据。
- `assets/app-bridge-entry.mjs:20` 和 `src/mcp.rs:125` 映射工具与路由，`src/mcp.rs:165` 返回工具错误。`src/runtime.rs:20` 设置超时，`:67` 区分持有服务与转发，`:75` 读取令牌并发出 POST。`src/service.rs:250` 校验实例身份后重用。
- `src/http.rs:30` 响应写入，`:107` 校验和派发。`src/archive.rs:11` 处理本次本机归档，`:45` 确认本地标记，`:100` 读回和写入原生端，`:177` 扫描已归档列表。
- `src/app.rs:173` 发布活动集合，`:202` 按集合过滤卡片。`src/app/native.rs:121` 核对原生归档但保留本地行，`src/store/native.rs:248` 事务提交本地归档标志。

真实环境仅执行只读核验。隔离试验使用临时目录、临时进程和测试 UUID。
