# PR #6 后台归属修复集成

owner: codex
execution_mode: coordinated_write
allowed_write_set: PR #6 合并及必要冲突解决、候选版本冻结、本任务记录与共享上下文
base_commit: 04f7d58f17a4301c8e5b7f50eae2e3bdeb62dddf
worktree_path: /Users/hongyong/.codex/worktrees/four-pr-rehearsal/codex-session-board
coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
status: active

用户授权检查并合并新 PR，同时观察现有流程的实际效果。
固定 PR #6 头为 966795746becd982ec1bdf1d2e6df735c369601e。
按既有集成规则保留七条原始提交，审查业务连接、状态保护、共享启动、测试及打包。
两个只读审查者分别检查 Rust 行为和启动/门禁兼容，协调者独占写入。

合并计时自 2026-11-14T13:00:20+00:00 固定来源开始，
到受保护主线确认同一候选且双平台不可变插件包及隔离启动检查通过停止。
记录同步及本任务分支清理单列耗时；发现、调查及此前流程开发不混入核心时间。
本次新增文件、工具及验收绑定按现有保守规则执行完整双平台门禁。
不安装或重载日用插件，目标环境验收另行安排，完整 31 条要求保持。

自动合并无冲突，七条来源提交及新增记录通过提交快检。
只读审查发现旧包启动验证将 fixture、serve 和 MCP 作为兄弟进程运行，
与本次强制父后台归属校验不兼容。已联系原开发会话
01a1d89d-57e0-71a2-9058-0775f92bc693 修复验证启动链，
要求保留归属、身份、MCP 与清理断言，双平台门禁仍由协调者统一执行。
候选尚未推送；初次未提交的版本准备已撤回，待修复合入后重新冻结。
修复及协调时间保留在本次总计时中，不重置时钟。
第二项审查发现继承实时后台后，本地侧栏写入仍可连接旧配置中的其他客户端。
已复核 native_reader、SidebarConnection 与 forwarding 的调用链并通知原会话，
要求写入前证明同一客户端来源，补错误来源零写入及匹配来源成功回归。
本地五项 rpc::authority 定向测试通过；远端推送通道 dry-run 通过，未更新远端分支。

源码外证据目录：
/Users/hongyong/.codex/visualizations/2026/11/13/01a1d38b-1ef8-7392-ba78-caa7935c5ab4/pr6-integration-20261114/
