# 统一验证入口

使用 `./scripts/validate`。R1 已统一环境选择、命令归属、运行记录和错误退出；R2 将实时连接构造限制在有归属的入口内。任务改动映射、浏览器场景编排和测试调度属于后续 R3/R8。

## 常用操作

```sh
./scripts/validate --scope candidate --output /tmp/board-candidate
./scripts/validate --scope preflight --output /tmp/board-source-preflight
./scripts/validate --staged --output /tmp/board-staged
./scripts/validate --scope candidate --plan
./scripts/validate --report /tmp/board-candidate
```

每次实际运行使用新的源码外目录，已有目录不会覆盖。`--staged` 可不传输出目录，工具会创建私有运行目录并打印位置。其他执行范围需要 `--output`；省略 `--scope` 仍执行 candidate。

`--plan` 只列出检查命令，不创建报告、编译缓存或执行检查。`--report` 只读取；第二版通过报告会重新核对阶段、日志和清理凭据。第一版报告仍可回看，但不能用于新候选晋升。

`candidate` 供发布前验证，在固定 `main` 提交上执行三步：

1. 项目规则检查。
2. 全部目标 Clippy，警告视为错误。
3. 完整自动验收，通过独立验收入口核对绑定、测试结果和需求。

日常合入执行本机项目规则检查和与改动相关的测试。具体顺序见 [本地合并与发布](INTEGRATION.md)。
发布 CI 汇总两个平台的完整候选报告和原始日志，核对源码后才打包。

preflight 检查便携源码包及指纹，不要求先安装 Rust。staged 只检查索引快照，并分别核对源提交、索引树和快照指纹；未暂存的修复不能使它通过。

## 环境

工具先保存未通过的初始报告，再选择 Python 3.11+。优先使用 `CSB_VALIDATION_PYTHON`，随后检查 PATH 和已有项目环境。不自动安装或更改全局环境。

```sh
CSB_VALIDATION_PYTHON=/path/to/python3.12 ./scripts/validate \
  --scope candidate --output /tmp/board-candidate-python
```

显式指定的解释器不可用时直接失败，不改用另一个版本。报告记录工具绝对路径和版本；子命令通过本次运行的工具入口继承同一 Python、Git、Cargo、Rust 和 Node。macOS 临时目录固定为 `/private/tmp`，其他平台使用规范化后的临时目录。
重载回归使用 `requirements-delivery.txt` 固定的 `websocket-client`。本机可将依赖安装在独立 Python 环境，再通过 `CSB_VALIDATION_PYTHON` 指定该环境。

普通验证默认使用本工作树 `target/validation/<工作树身份>`，凭据绑定源码目录的真实路径、设备和目录身份。同一路径删除后重建也使用新缓存。显式 `CARGO_TARGET_DIR` 中已有内容却没有归属凭据，或归属其他源码副本，均拒绝使用；请提供新的空目录，不要删除或伪造凭据。暂存快照使用自己的临时 target，不继承工作树缓存。因此新副本和暂存完整检查有独立编译成本；开发提交仍使用 `./scripts/check --pre-commit` 快检。

### 固定工具缓存

`scripts/check` 自动经现有 Cargo 入口构建独立的 `scripts/checker/Cargo.toml`，
复用原 `project_check.rs`，不再为了运行固定检查器编译整个业务 crate。
缓存默认在 `${XDG_CACHE_HOME:-$HOME/.cache}/codex-session-board/checker`，
可用 `CSB_CHECKER_CACHE_DIR` 指定。三个工作流继续使用 actions/cache 保存该目录。
每次仍在当前工作树或索引快照实际执行检查；命中只表示工具无需重新编译。
CI 与 `dev setup` 的现有在线准备步骤分别 fetch 根锁和工具锁，之后继续离线构建。
两份锁可以独立演进，不依赖缓存中碰巧存在的旧依赖。手动准备环境同样执行两次 fetch。

| 程序 | 复用边界 |
| --- | --- |
| 固定项目检查器 | 工具源码、独立锁、实际 Cargo/Rustc、平台、配置和编译条件相同才复用不可变二进制 |
| 检查器与 hooks 自测 | 工具改动仍由现有 Rust 测试运行；普通业务范围不重复执行无关工具自测 |
| 业务库及 Rust 测试驱动 | Cargo 在当前候选的独立业务 target 中构建和执行，不复用其他候选的测试可执行文件 |
| release fixture | 依赖业务代码，按固定发布源码构建；只在发布阶段使用 |
| Python 与 Node 检查 | 保持现有解释执行入口，不另建测试调度器 |

缓存条目校验输入身份和二进制摘要，私有暂存后原子发布。
同构建条件只锁构建过程，等待者读回同一条目；不同构建条件使用独立 Cargo 目录。
工具输入变化自动生成新键，损坏条目拒绝运行，不会静默作为验证成功。
配置直接指定的编译器、wrapper 和 target linker 同样按内容失效。
当前不支持 Cargo config include、配置 env 重定向工具/PATH、非字符串工具引用，
以及 rustflags 中的外部 linker 引用；这些情况明确拒绝复用，而非执行身份不明的缓存。
`CSB_CHECKER_CACHE_REPORT=/absolute/outside/source.json` 可记录键、是否命中、
二进制摘要和解析/构建耗时；完整进程耗时另计，不能把工具级收益外推为整条流水线。

## 结果和清理

`VALIDATION.json` 第二版包含来源提交/树/指纹、工具环境、执行计划、逐步耗时、首次失败、日志摘要、待验证项和清理结论。准备及收尾位于独立字段，候选三个 `steps` 的含义不变。标准输出和错误输出分别保存；失败时仍在终端显示相应诊断。

自动阶段失败但已生成匹配源码的报告时，仍记录其中的待验证项和摘要。报告分析失败不会覆盖最初的命令失败，缺少报告也不表示目标环境已经验收。

每条命令都有独立守护进程、进程组、租约和清理回执。父验证进程正常退出、失败、超时、中断或被强杀时，管道关闭会触发清理；只操作本次持有的进程组，不根据历史 PID 杀进程。单步默认超时 1800 秒，可用 `--timeout` 显式调整。

守护者自身被强杀或系统断电时，不能保证即时回执；报告保持未完成，不会补造清理通过。父进程被强杀后，主报告可能停留在运行中，应读取对应 `step-N.lease.json` 的最终结果。R1 的归属范围是命令进程组；另起会话的浏览器、真实客户端等资源由后续场景编排登记和核验。

| 退出码 | 含义 |
| --- | --- |
| 0 | 所请求的自动范围通过 |
| 1 | 规则、断言或来源一致性失败 |
| 2 | 保留给要求外部证据的范围；当前自动范围不以此表示真实环境待验 |
| 3 | 环境、工具、超时或清理错误 |
| 130 | 验证被中断 |

首次失败保留。`scope_passed`、`release_ready` 和目标验收状态独立。发布流水线核对第二版阶段、日志摘要、进程归属、清理凭据、自动报告及平台，再检查实际包的隔离启动和故障场景。

## 连接边界

业务和集成测试使用 `App::connection`。`configured` 仅接受 app 内独立 connection 模块能够构造的凭证；app 的其他业务子模块也不能伪造。原始 connect/proxy 不作为生产 API，remote 仅在 rpc 内可见。预检属于 connection 子模块，并保持原来的公开预检路径。

低层 fixture 客户端只放在 examples/tests 中；历史 `StdioReader` 的用途保持不变。自动门禁先编译正向样例，再在源码外副本的 crate 兄弟模块及 app 业务子模块分别注入六种绕过，逐项核对十二处错误位置和类别。别名或空格不能绕过 Rust 类型和可见性约束。
