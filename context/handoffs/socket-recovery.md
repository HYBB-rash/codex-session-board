# 客户端中断恢复交接

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

尚待实际桌面闪退重启及新阶段自动移列的端到端验证。
桌面主进程拥有 app-server，退出后的 orphan-reaper 会清理 authority，
当前轮次无法保证存活。不能暂停清理器或改 authority 归属伪装成正常生命周期。
后续需要先准备独立编排、自动重启和证据观察进程，再做实际桌面强退；
恢复后独立读回原生位置并与看板状态核对。不能把本次地址换代当成真实桌面闪退。
后续 0.1.77 为部署记录提交；不要构建并覆盖日常的精确 0.1.76。
独立编排脚本已准备为 `outputs/socket-recovery-deployment-20261107/desktop_crash.py`。
无参数仅预检，`--run` 才执行真实桌面 SIGKILL、独立服务启动桌面与恢复观察。
预检已核对唯一实际桌面 socket 对端、当前轮次运行中、无其他活跃本地用户聊天。
在启动实际验证前完成提交和会话检查，随后由 systemd 用户临时服务接管，
桌面会自动重开，当前轮次预计中断。机器报告为 `desktop-crash-recovery.json`，
简报为 `desktop-crash-result.txt`，实际结果以报告 passed 和独立原生读回为准。
恢复聊天后应先读此报告；不可因脚本已启动就把真实闪退记为通过。
31 条契约、参考模型与锁保持不变，缺少外部证据不签收完整验收。
