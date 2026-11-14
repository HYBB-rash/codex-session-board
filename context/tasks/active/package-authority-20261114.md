# PR 6 集成兼容与侧栏归属修复

- owner: codex-original-pr6
- execution_mode: isolated_write
- allowed_write_set: 本任务记录；fixture 托管、实际包启动验证、侧栏归属及相关回归
- base_commit: 865fb97
- worktree_path: /Users/hongyong/.codex/worktrees/package-authority-fix/codex-session-board
- coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
- status: active

固定主线 04f7d58 与原 PR 头 9667957 无冲突组合为签名复现基线 865fb97。
先让包内 serve 与 MCP 均由监听端点的同一 fixture 托管，保留原有身份、就绪、协议与清理断言。
随后封闭实时后台与侧栏来自不同客户端的写入边界；不能证明归属时，在任何写入前拒绝。
不修改协调者工作区、共享 STATE、版本或日用客户端。仅执行针对性验证；完整双平台候选由协调者统一执行。

包托管实现检查点 9ac11e8；侧栏归属与共享客户端证明检查点 af7082a。针对性第二轮
81 项 RPC、37 项原生、3 项 fixture、29 项 Python 及相关 Clippy 通过；另 49 项远程回归通过。
侧栏首轮新增回归发现上层把来源拒绝包装为移动未确认，已将来源校验提前到连接阶段并复验，
保持原生成功路径与错误来源零字节断言。共享兄弟进程必须有后台、GUI、启动器三方身份及
实际父子关系；普通 stdio 客户端按后台与 GUI 父子关系核对。旧 epoch 和重启拒绝已有回归。

新增凭证入口的真实 Mac 私有验证通过，后台 98759、GUI 98770、启动器 98731；清理确认。
首个实际包 af7082a 已构建、归档并解包；HTTP 和 MCP 均成功，但比较身份时误把 HTTP 专用
requestToken 当作共享身份，导致验证拒绝。失败及 cleanup_confirmed=true 保存在
/private/tmp/csb-pr6-package-proof-20261114-r1/success.json。改为严格核对七项实际身份字段，
包括后台 PID/epoch、服务 PID、构建、配置及数据目录；不把传输令牌保存为后台身份。
实际包完整正例及故障注入仍待下一轮完成。协调者指出的 HTTPConnection 上下文协议问题
已按既有 closing 模式修正，异常原始 error 与清理字段继续保留。
