# 官方原生入口 PR 作者任务

owner: codex
execution_mode: serial_write
allowed_write_set: 本任务记录、原生入口范围内的审阅修复、源码外 PR 状态及独占巡检单元
base_commit: 9e2f64e4010787338a6a61851a3a4a07bf6dafc4
worktree_path: /home/herman/.codex/worktrees/official-native-entry-pr/notion-program
coordinator: 01a1e3f0-8b42-7b72-b57d-62b1049160e5
status: completed

用户显式调用 pr-task，授权验证、推送、草稿 PR、后台巡检及范围内有限修复。
从 f5b3ff5 的完成实现创建作者分支 codex/official-native-entry-20261117，
重放到后续主线 9e2f64e，保留新增验证缓存、合并/发布分离及版本 0.3.98。
原目录的 main 保留原头，不重置已有工作。

冻结检查器为 /home/herman/.codex/worktrees/official-native-pr-checker/notion-program，
固定 9e2f64e、Linux x86_64；队列须沿用同一检查器身份和平台。
运行记录为 /home/herman/.local/share/codex-session-board/pr-tasks/official-native-entry-20261117。
原会话沿用当前 UUID，CLI 核对继承的私有共享后台；不猜测其他端点。
本会话不登记自动归档；Linux 使用外部纯代码定时器调用冻结 CLI。
默认每十分钟，最多三轮修复、七十二小时，不自动增加额度。

提交前执行新的 submit 阶段；旧 0.3.97 的 989 项及六个启动场景仅保留为历史证据，
不得代签新主线上的源码、完整 PR CI 或目标环境验收。不安装或重载日用插件。

独立业务审阅在 44f18d3 发现新增 metadata/update 未进入远程写入前来源复核。
已中断该头的 submit 验证，不发布旧头。补入与其他远程变更相同的来源复核，
使用确实允许写入的 ProjectWriter 做正向对照，再分别改变 alias 解析、UID、
HOME 和 CODEX_HOME，验证后续更新被拒绝且没有新增写入事件。

## 合并协调者收尾

PR19最终作者头62d54db894590b743832b2055cf9857159baa75b经原两个独立审阅者复核，
原冻结检查器的submit报告和准确头PR CI43912935654均通过。
最终组合a34cb075811a417be63cc69aa3cf8285ef05d763/0.3.99经
CI43916778874 attempt3双平台1046/1057项通过，于2026-11-16T18:04:42Z确认进入main。
两次macOS容量排队失败没有改变源码，Linux成功任务复用，原失败保留。
作者任务于18:05:37Z读回complete、result=merged、rounds=0、last_error=null；
现有外部watch.py完成本任务systemd timer/service文件删除及停用读回，
external-monitor.status=removed。未请求原生归档，不归档原会话。
自动发布独立，未安装或重载日用插件，真实客户端验收仍待单独执行。
