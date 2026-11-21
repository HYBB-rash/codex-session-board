# C2 在线与离线缓存构造调查

owner: codex/refactor-c2-remote-construction
execution_mode: isolated_write
allowed_write_set: src/app.rs, src/remote/instances.rs, src/remote/cache.rs, src/remote/c2_construction_tests.rs, acceptance/bindings.json, context/tasks/active/refactor-c2-remote-construction-20261121.md, context/results/refactor-c2-remote-construction-20261121.md
base_commit: f15754129ac64a1dd02ec93abb00f9237bf0c9f8
worktree_path: /Users/hongyong/.codex/worktrees/b3f7/codex-session-board
coordinator: Herman / 01a1fc2d-af6f-7d50-9759-33835b51eaeb

2026-11-21 首次 fetch 后起始干净，HEAD 与 origin/main 均为 b859880165908a848a4982d2cfd9b722e9aeb912。
从 origin/main 建立本任务分支，不复制 C1，也不操作其作者分支。
C1 只读参考固定为 f15754129ac64a1dd02ec93abb00f9237bf0c9f8。
收到协调者 C1 合入通知后再次 fetch，读回 origin/main 为该 C1 头，再成功 rebase。
当前基线为上列 base_commit，仅有本任务两个新增上下文文件。

首件先列在线、离线缓存的全部 App 字段、创建者、使用者和归属。
核对断线、重连、重启读旧缓存和来源变化后，再决定是否存在可净删的独立修改。
若拆分只增加层次，调查及独立复核即结项；若依赖 C1，明确记录并保留新主线续作入口。
保持外部协议、持久格式、冻结模式，不更改共享 STATE、journal 或任务登记。
由协调者登记和归档任务。

实施采用 Coding Taste；沟通采用 Unslop、Write Report。
Parallel Subagent Work 用于独立只读调用链调查与最终独立审阅。
子代理不改源码、索引、分支、共享上下文或测试输出。

不新增测试或认领共享测试 ID，不接管 C1 归档测试。
每次提交先审阅实际索引并运行 ./scripts/check --pre-commit，至多 13 文件、400 行。
最终运行 ./scripts/check 与适用测试，不推送、不合入 main、不安装或部署。

首件及字段表在 context/results/refactor-c2-remote-construction-20261121.md。
22 字段中 5 个根业务专用、16 个按主机隔离、1 个在线共享根库。
在线/离线配置分别覆盖 7/5 项，只有主机和目录两项同义赋值。
没有找到可独立净删的初始化组；拆根/主机类型需要迁移现有调用者。
C1 的 HostArchive 仍依赖目标 App，调查结论不依赖它尚未合入时的接口。

两位只读子代理分别调查并复核，修正生命周期描述后保持不拆分结论。
未改业务源码、测试、绑定、版本或任何外部协议和持久格式。
原基线及 rebase 后 C1 基线各完成相关 66 项回归，全部通过。
原沙箱失败与各次授权重跑日志均保留。
context 校验通过；最终提交快检、干净树检查及准确提交头随回执交协调者。
调查完成，任务登记、归档和共享状态更新继续由协调者负责。
