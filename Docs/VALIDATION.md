# 统一验证入口

使用 `./scripts/validate`。R1 已统一环境选择、命令归属、运行记录和错误退出；R2 将实时连接构造限制在有归属的入口内。任务改动映射、浏览器场景编排和测试调度属于后续 R3/R8。

## 常用操作

```sh
./scripts/validate --scope pr --base origin/main --output /tmp/board-pr-review
./scripts/validate --scope candidate --output /tmp/board-candidate
./scripts/validate --scope preflight --output /tmp/board-source-preflight
./scripts/validate --staged --output /tmp/board-staged
./scripts/validate --scope candidate --plan
./scripts/validate --report /tmp/board-candidate
```

每次实际运行使用新的源码外目录，已有目录不会覆盖。`--staged` 可不传输出目录，工具会创建私有运行目录并打印位置。其他执行范围需要 `--output`；省略 `--scope` 仍执行 candidate。

`--plan` 只列出检查命令，不创建报告、编译缓存或执行检查。`--report` 只读取；第二版通过报告会重新核对阶段、日志和清理凭据。第一版报告仍可回看，但不能用于新候选晋升。

PR 范围保留现有相关回归映射；基于其他 PR 开发时显式传入实际基线分支。它不替代完整候选验证。candidate 保留以下三个步骤及顺序：

1. 项目规则检查。
2. 全部目标 Clippy，警告视为错误。
3. 完整自动验收，通过独立验收入口核对绑定、测试结果和需求。

preflight 检查便携源码包及指纹，不要求先安装 Rust。staged 只检查索引快照，并分别核对源提交、索引树和快照指纹；未暂存的修复不能使它通过。

## 环境

工具先保存未通过的初始报告，再选择 Python 3.11+。优先使用 `CSB_VALIDATION_PYTHON`，随后检查 PATH 和已有项目环境。不自动安装或更改全局环境。

```sh
CSB_VALIDATION_PYTHON=/path/to/python3.12 ./scripts/validate \
  --scope candidate --output /tmp/board-candidate-python
```

显式指定的解释器不可用时直接失败，不改用另一个版本。报告记录工具绝对路径和版本；子命令通过本次运行的工具入口继承同一 Python、Git、Cargo、Rust 和 Node。macOS 临时目录固定为 `/private/tmp`，其他平台使用规范化后的临时目录。

普通验证默认使用本工作树 `target/validation/<工作树身份>`，凭据绑定源码目录的真实路径、设备和目录身份。同一路径删除后重建也使用新缓存。显式 `CARGO_TARGET_DIR` 中已有内容却没有归属凭据，或归属其他源码副本，均拒绝使用；请提供新的空目录，不要删除或伪造凭据。暂存快照使用自己的临时 target，不继承工作树缓存。因此新副本和暂存完整检查有独立编译成本；开发提交仍使用 `./scripts/check --pre-commit` 快检。

## 结果和清理

`VALIDATION.json` 第二版包含来源提交/树/指纹、工具环境、执行计划、逐步耗时、首次失败、日志摘要、待验证项和清理结论。准备及收尾位于独立字段，候选三个 `steps` 的含义不变。标准输出和错误输出分别保存；失败时仍在终端显示相应诊断。

每条命令都有独立守护进程、进程组、租约和清理回执。父验证进程正常退出、失败、超时、中断或被强杀时，管道关闭会触发清理；只操作本次持有的进程组，不根据历史 PID 杀进程。单步默认超时 1800 秒，可用 `--timeout` 显式调整。

守护者自身被强杀或系统断电时，不能保证即时回执；报告保持未完成，不会补造清理通过。父进程被强杀后，主报告可能停留在运行中，应读取对应 `step-N.lease.json` 的最终结果。R1 的归属范围是命令进程组；另起会话的浏览器、真实客户端等资源由后续场景编排登记和核验。

| 退出码 | 含义 |
| --- | --- |
| 0 | 所请求的自动范围通过 |
| 1 | 规则、断言或来源一致性失败 |
| 2 | 保留给要求外部证据的范围；当前自动范围不以此表示真实环境待验 |
| 3 | 环境、工具、超时或清理错误 |
| 130 | 用户中断 |

首次失败保留，不自动重试至通过。`scope_passed`、`release_ready` 和目标验收状态独立；自动范围通过仍可能有真实环境待验。候选晋升同时核对第二版阶段、日志摘要、进程归属及清理凭据，并继续核对原有自动报告、平台和包启动证明。

## 连接边界

业务和集成测试使用 `App::connection`。`configured` 仅接受 app 模块能够构造的凭证；原始 connect/proxy 不作为生产 API，remote 仅在 rpc 内可见。预检属于 app 子模块，并保持原来的公开预检路径。

低层 fixture 客户端只放在 examples/tests 中；历史 `StdioReader` 的用途保持不变。自动门禁先编译正向样例，再在源码外副本注入六种业务模块绕过，逐项核对错误位置和类别。别名或空格不能绕过 Rust 类型和可见性约束。
