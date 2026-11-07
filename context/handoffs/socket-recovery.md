# 客户端中断恢复完成记录

源码位于 `/home/herman/.codex/worktrees/socket-recovery/notion-program`，
分支 `codex/socket-recovery`，基于干净的 `5f95c6b` 隔离开发。

- `7b78f40`：旧 socket 文件残留且拒绝连接时重新发现唯一存活端点。
- `2c3ec67`：已接受阶段跨服务重启保留。
- `8054156`：延迟读回按自动目标确认，同轮新阶段可接替旧 pending。

三步完整提交钩子与提交后检查通过。只读实测使用独立数据目录，
关闭原生分区写入、标题同步与桌面执行观察，保留真实失效旧地址，
成功自动发现连接并读回 41 条远程会话，未重写配置。
证据保存在 `outputs/socket-recovery-20261107/`。

0.1.76 完整冻结门禁已执行，316 项自动检查通过，17 条需求通过、14 条既有外部
验收缺证，退出 2，release_ready=false。正式报告为
`outputs/socket-recovery-20261107/full-acceptance.json`；
同版本只读实例的旧地址恢复记录为 `readonly-live-recovery.json`。
用户后续授权部署精确 0.1.76。已从 a44bf20 构建 release 程序，运行构建为
`40db051d6b227788a53491c4757da636fdd786f86bc615154a8b595fba59b223`。
HTTP、插件与 MCP 均为 0.1.76，日常、marketplace、缓存及运行文件四处哈希一致。
现有配置字节、MCP 环境和 data 路径保留。停止旧服务前后各有 SQLite 一致性备份；
本地 130 条、远程 41 条记录的人工字段与设置未变。备份及证据位于
`outputs/socket-recovery-deployment-20261107/`，原 checkout 同路径另有副本。

日常看板真实 SIGKILL 后以受管 launch 重启，约 1 秒恢复健康。
此过程是手动触发受管启动，不是新增服务自动拉起机制；此前无 pending，
pending 重开及新阶段接替仅由回归测试覆盖。
真实监听 socket 改名、旧址留 ECONNREFUSED 文件的地址换代注入中，
看板进程未重启，配置未修改，约 5.54 秒恢复读取，原 socket 路径已恢复。
release 模式 26 项原生权威测试和真实浏览器页面及周期刷新通过。

真实桌面闪退重启及新阶段自动移列的端到端验证随后完成。
独立 systemd 用户临时服务运行 `desktop_crash.py --run`，预检当前轮次运行中、
没有其他活跃本地用户聊天，备份看板 SQLite、桌面 state_5.sqlite 和 global state。
2026-11-07 18:22:22 对实际 socket 对端桌面进程 532421 发送 SIGKILL。
18:22:28 请求正常重开，18:22:44 验证通过。旧 socket 仍存在且 errno 111，
新桌面进程为 1510409，socket 换为新的 UUID 文件。
日常看板 PID 1434679、构建、配置字节和 configId 均未改变，
启动桌面后约 15.40 秒自动恢复连接和同步，既有一条历史原生位置缺项提示保留。

同一真实轮次从 inProgress 变为 interrupted，自动移入已阻塞，
原生 thread/read 独立读回已阻塞分区，自动日志序号为 607。
恢复期间确实观察到 Broken pipe、连接不可用和原生移动未确认，
这些错误随后自动清除，没有手工换配置或重启看板。
130 条本地、41 条远程人工字段与设置保留，原生人工选择锚点不变，
输入版本不倒退，既有复核标志不清零。
机器报告 `desktop-crash-recovery.json` 的 passed=true 已读回审阅，
简报为 `desktop-crash-result.txt`，两份 checkout 的部署目录都有报告。

后续新轮次 01a1b58a-802d-7070-9a1e-374ad5f981d3 再次执行时，
自动变更日志 609 将卡片放回进行中，当前真实阶段、看板和原生独立读回一致。
`resumed-phase-verification.json` 和 `frontend-after-real-crash.json` 保存恢复后的证据。
保全结论针对 18:22:44 验收结束时；目标会话在 18:23:35 有后续确认锚点变化，
新阶段继续执行，不以早前基线阻止合法后续操作。独立只读审阅确认专项通过，
其余 129 条本地记录受保护字段仍一致。远程既有输入版本均为 0，
本次不声称已覆盖非零远程输入版本恢复或 live pending 跨看板重启重放。

本次部署和故障恢复任务已归档到 `context/tasks/completed/socket-recovery.md`。
文档记录 0.1.77、0.1.78 不部署，日常继续运行用户指定的精确 0.1.76。
没有安装看板进程自动拉起机制；实际看板 SIGKILL 测试使用受管 launch 重启。
真实桌面测试包含独立编排请求重新启动，不表示新增了桌面永久自动重启机制。
31 条契约、参考模型与锁保持不变，缺少外部证据不签收完整验收。
