# 改用系统临时目录清理

owner: codex
execution_mode: serial_write
allowed_write_set: context/, Docs/SOCKET-LIFECYCLE-REPORT-20261111.md, Cargo.toml, Cargo.lock, outputs/system-tmp-cleanup-20261111/, /etc/tmpfiles.d/tmp.conf, /etc/systemd/system/systemd-tmpfiles-clean.timer.d/override.conf, 原两套tmp-autoclean单元与脚本
base_commit: b8d385597993077f745db37c5c023f0e65d81c27
worktree_path: /home/herman/文档/notion-program
coordinator: codex

用户明确授权删除用户级及系统级自定义 timer，改用系统清理程序。
每十五分钟检查一次，/tmp 老化门槛四小时；/var/tmp 保留原三十天。
删除两套旧 timer 及其配套 service、脚本，验证单元文件和调度均不存在。
准备受限的管理员执行脚本，通过桌面 PolicyKit 认证，不索取或记录密码。
隔离验证活 socket 保留、过期普通文件可清理，核对实际配置及系统服务结果。
不重启 Codex，不承诺已丢失端点恢复或客户端生命周期修复完成。

2026-11-11 03:04 已通过桌面 PolicyKit 管理员认证执行，无密码进入命令或日志。
删除用户及系统两套 tmp-autoclean.timer、配套 service 和脚本，以及两个启用链接。
两个管理器查询均为 LoadState=not-found、ActiveState=inactive，无匹配 unit 文件。
系统原生清理程序保持不变，新增 /etc/tmpfiles.d/tmp.conf 覆盖 /tmp 为 4h，/var/tmp 保留 30d。
新增默认 timer 的 override.conf，清空原触发配置后设开机十五分钟及运行后每十五分钟，精度一分钟。
03:04:19–03:04:20 首次实际清理 Result=success、ExecMainStatus=0；下一次为 03:19:19。
隔离实测过期普通文件删除、活 socket 保留，清理后新连接与收发成功。
桌面 PID 2505572 与 authority PID 2506017 及启动时间保持不变。
执行脚本、原文件校验和、配置、预演日志和验证结果保存在 outputs/system-tmp-cleanup-20261111/。
未保留旧单元或脚本的备份副本，旧历史清理日志未删除。
已完成清理策略切换，不宣称已丢失 socket 恢复或所有生命周期问题解决。
