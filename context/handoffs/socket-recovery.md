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
日常看板仍运行 0.1.72，本任务未部署、未改变日常配置或数据。
后续部署需先核对当前 checkout、服务与插件构建，备份并保留配置和人工字段，
不能只替换 socket 后把代码恢复机制记为已部署。
31 条契约、参考模型与锁保持不变，缺少外部证据不签收完整验收。
