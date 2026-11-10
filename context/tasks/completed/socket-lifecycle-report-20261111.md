# 分区 socket 反复失效的长期解决报告

owner: codex
execution_mode: coordinated_read
allowed_write_set: Docs/SOCKET-LIFECYCLE-REPORT-20261111.md, context/, outputs/socket-lifecycle-report-20261111/, Cargo.toml, Cargo.lock
base_commit: ff64c3e270d5a16c4ab26f19491a52818b6c20b7
worktree_path: /home/herman/文档/notion-program
coordinator: codex

用户要求完整报告，能够指导找到长期解决分区 socket 反复不存在的方案。
主代理整理既有故障、现场与接口证据，协调三项只读调查：
客户端端点生命周期、系统清理与删除证据、替代写入接口及维护边界。
报告明确已证实事实、未证实原因、推荐方案、实施顺序和故障验收矩阵。
不修改桌面安装包、正式配置、生产数据库，不重启客户端，不移动用户会话。

报告已完成，见 Docs/SOCKET-LIFECYCLE-REPORT-20261111.md。
已定位用户及系统两套每 15 分钟执行的四小时清理器，确认顶层 mtime 和 fuser 判断不能保护内部活 socket。
隔离实测复现清理候选误判及 unlink 后旧连接仍活、新连接 ENOENT、owner 重绑恢复。
11 月 10 日用户清理结束后约十秒出现该地址首条 ENOENT，历史删除责任 PID 仍缺目标级审计。
安装源码确认 app-tools 缺少运行期重绑，社区 app-server 桥在 authority 活着时跳过路径复查。
报告明确真实监听路径、维护仓库及 updater 交付边界、公开分区 API 的当前证据和受控写入门槛。
只读独立审阅通过归责措辞，并已补齐 timer 停用、原删除条件和进程任务连续性的验收断言。
私有证据、校验和及可重跑隔离脚本保存于 outputs/socket-lifecycle-report-20261111/，未纳入 Git。
本次仅交付调查与长期方案，外部脚本、timer 和生产端点均未修改，不宣称修复完成或 release_ready。
