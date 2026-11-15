# PR #8 历史队列冲突修复

- owner: 原作者会话 01a1da15-3bcd-7e32-9033-8d22bdfb545f
- execution_mode: isolated_write
- allowed_write_set: 指定修复工作树中的验证入口、晋升冲突、相关回归及绑定、验证说明和本任务文件
- base_commit: 36fed3a74d606ecc6c02f0ae0ad677eb581f61c9
- worktree_path: /Users/hongyong/.codex/worktrees/pr-queue-author8-20261115/codex-session-board
- coordinator: 历史四 PR 合并队列会话 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
- status: in_progress

反馈 b6d2ca66-0e8f-5f13-bcd2-62f934399043 指定只合入组合前置提交
22b9dc4e45c872d91e921c2f636d256001241ac6。保留原 PR 头祖先关系；
不合入其他候选或当前主线，不修改正式插件及日用客户端。

merge-tree 已复现 promote_candidate.py 与 validate_candidate.py 两处冲突。
首次输出保存在源码外 author-8/merge-tree-initial.log，不覆盖。
保留前置提交的 PR 交付入口、固定源码和实际包检查；候选继续执行 R1 第二版
阶段、工具、进程归属及报告校验。演练引用、产物标签与发布渠道隔离保持。
版本、共享 STATE、需求、独立模型与锁只继承指定前置提交，不另作修改。

完成源码提交后写 author-8/ready.json，由协调者另行分派独立审阅。
使用固定 ec4e9c6 检查器对最终源码头和 22b9dc4 固定基线重新验证。
所有 Cargo/check/validate 设置 CARGO_BUILD_JOBS=2，报告使用源码外的新目录。
最终结果写 author-8/result.json；未收到独立审阅结果不得代写通过。

冲突解决已保留双平台晋升校验与演练引用隔离，PR 参数交给原交付流程初始化，
避免入口提前创建目录或注入候选凭据。新增两个真实 CLI 回归覆盖固定外部源码计划、
错头拒绝、旧报告不覆盖、范围混用和预算绕过拒绝；原测试及断言保留。
59 项入口、PR 交付、候选、晋升、演练目标与产物回归通过，耗时 12.456 秒。
首次直接调用的 python3 是 Python 3.10，缺少 tomllib/contextlib.chdir；
该失败保留在 focused-initial.log，已有 Python 3.13 的新结果为 focused-python313.log。
完整固定检查器验证和独立审阅仍待最终源码提交后执行，结果以源码外回执为准。
