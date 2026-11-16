# 官方原生入口同步主线

owner: codex
execution_mode: serial_write
allowed_write_set: 官方入口相关源码、测试、文档与本任务记录
base_commit: 1ba252a
worktree_path: /home/herman/文档/notion-program
coordinator: 当前会话
status: completed

用户要求将现有实现 rebase 到最新主线。原头 8c5b2f1，
备份引用 refs/codex-backups/official-native-before-rebase-20261116。
主线固定为 1ba252a，保留 0.3.97、PR 队列工具、测试并行设置及共享 STATE。
保留项目准备、来源/账户校验和稳定操作重试，导航改用官方 SSH 单目录入口。
移除主线新增的客户端修改脚本及其专属测试，替换为官方入口行为验证。
本任务不推送、不安装或重载日用插件。

2026-11-17 完成重放及验证。主线项目准备、幂等导入、账户与来源校验保留；
最终导航使用官方 SSH alias 和单目录入口，实际 UUID 归属补登记仍独立确认。
旧项目导入后，无论目录尚未刷新还是已转换成规范项目，都复用同一 operation。
浏览器与 Rust 回归覆盖旧项目 key 消失后的重试，不重建会话或发送输入。
修改客户端的脚本及专属回归已退休，后台项目准备与权限保护测试继续保留。
保留主线版本、共享 STATE、PR 工具与测试并行设置；需求、模型与锁未改。

重放后两条提交因新增冲突解决超出规模限制，已拆分；全部本地提交保持
最多 13 文件、400 行，每次新提交均经过暂存快检。拆分中一次缺少旧路由符号的
中间态被快检阻止；将官方导航调用与入口一起前移后正常通过，未绕过检查。

首次完整报告 validation/ 为 987 通过、2 失败：主线 fixture 延迟 1.1 秒写回执，
两个移入断言仍立即读文件。改用主线已有 native_navigation_receipt 后，
五条原生入口定向测试通过；原断言与延迟保留。
失败报告不删除，不作为当前通过证据。

最终运行 ./scripts/validate --scope pr --base 1ba252a，
报告位于 /home/herman/.local/share/codex-official-native-rebase-20261116/validation-fixed/。
源码头 38f7bb68c8a45452bff477d8241da37dc5b1fabc；
实现指纹 9b3f135c322fc107fdaf44b96752bf12374b401bbb9358de83415427e504ddeb。
Linux 989 项自动测试全过，not_run=0；全部目标 Clippy、源码包及六个实际插件包
隔离启动场景通过，清理确认通过，总计 475.451 秒。
本次只签收本地自动组合，release_ready=false；目标环境及完整验收独立。
收尾记录不改变实现指纹，结束前执行 ./scripts/check 与上下文检查。
