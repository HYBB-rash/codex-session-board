# 调查远程分区映射的其他接口与确认路径

owner: codex
execution_mode: isolated_write
allowed_write_set: Docs/REMOTE-MAPPING-SOURCE-20261114.md、当前任务记录、源码外 /private/tmp/csb-mapping-source-20261114 实验资料
base_commit: 6c530f5606f35ed4d3fb45888e25b91f42ef6c9b
worktree_path: /Users/hongyong/Developer/codex-session-board-live-mapping-20261114
coordinator: codex
status: completed

用户要求带着可靠远程分区的需求查文档、Codex 源码，发现突破口后进行验证。
调查完整映射读取、原生分区接口和确认语义，不只停留在 list_threads 的字段限制。
优先只读验证；若需写入实验，只操作本任务创建的空会话、分区或隔离数据，
随后清理并核对，不改日用客户端、已有会话、自动分区开关、版本或共享 STATE。
成功标准是区分已验证可用的能力、受限能力和未验证的可靠性条件，给出可执行判断。

已读取当前官方文档、冻结上游源码和安装包源码，并在 Linux → M1 上验证：
已有五个映射通过目录检查；两个同名新空分区在映射尚未落盘时可分别校准；
删除重建产生新 ID，旧映射可判失效；八次复用移动均正确，合计收到十一条对应通知。
未注入延迟。移动后实际位置读回为 2.10–5.33 ms，不代表全流程或完整初始化耗时。
缺失映射的受控校准有实现依据，但真实新主机、账号切换、断线与并发恢复仍待验证。

报告：Docs/REMOTE-MAPPING-SOURCE-20261114.md。
证据：/private/tmp/csb-mapping-source-20261114/evidence-sha256.json。
独立重连确认空会话已归档且零轮次，临时逻辑与后台分区已清理；
既有五个分区、十个映射、十六个成员键相同，临时连接和 agent 均退出。
没有修改业务代码、日用配置、自动分区开关、版本或共享 STATE。
