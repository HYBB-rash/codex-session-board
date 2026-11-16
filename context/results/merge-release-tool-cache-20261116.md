# 固定检查器缓存实现

owner: codex-tool-cache
execution_mode: isolated_write
allowed_write_set: scripts/check; scripts/checker/**; scripts/checker_cache.py; tests/git_hooks.rs; scripts/package_migration.py; acceptance/test_checker_cache.py; 本记录
base_commit: aa1f5663eb09add4aef2624b070935462d25342b
worktree_path: /Users/hongyong/.codex/worktrees/merge-release-tool-cache-20261116
coordinator: /root
status: complete

固定检查器继续复用现有Rust源码、CLI和测试，不改变业务target归属或测试并发。
独立Cargo manifest只构建固定工具，当前候选的clippy、Rust测试构建仍执行根build.rs。
工具依赖锁、源码和构建条件固定后缓存不可变二进制，只锁构建过程。

接口已完成：`scripts/checker_cache.py`；默认缓存 `$XDG_CACHE_HOME/codex-session-board/checker`，
覆盖变量 `CSB_CHECKER_CACHE_DIR`，证据路径变量 `CSB_CHECKER_CACHE_REPORT`。
独立清单和锁位于 `scripts/checker/`；当前已提交 `1fd9a3e`、`00217ee`、`34d4222`。
协调者需要将 `scripts/validate_staged.py` 的第一条旧 `cargo run --example project_check`
改成现有 `./scripts/check --pre-commit --baseline-repository ...`，否则它仍会要求 fixture
重新编译内嵌检查器。该文件不在本子任务允许写入集合，保留给协调者。

实现提交（按顺序）：

- `1fd9a3e32faecb933acd02b35e830c092925dd8e`：建立独立工具清单及核心依赖锁。
- `00217eee6f4f2d760dd1ad95225ee93c41993e35`：补齐现有 CLI、词法和策略解析依赖锁。
- `34d4222493aeba4b1962b5f642e472994e3da2a6`：内容缓存、入口接入和源码包文件集合。
- `237a8a5b5b19193d8d91d2333a8718380d341bd3`：fixture 复用及 8 项缓存回归。
- `8c88ee4fa566234f61d59de467912c6de3defa72`：指纹使用 rustup 实际选择的 Cargo/Rustc 二进制。

最终实测证据根目录：
`/Users/hongyong/.codex/visualizations/2026/11/16/merge-release-tool-cache-20261116`

`complete-summary.json` 使用最终工具源码和真实 Cargo 构建：

|情况|缓存解析/构建耗时|完整进程耗时|结果|
|---|---:|---:|---|
|首次构建|8.282 秒|9.279 秒|未命中，产生带摘要的不可变工具|
|再次调用|0.226 秒|0.321 秒|命中，键和二进制摘要相同|
|另一个源码目录|0.227 秒|0.282 秒|命中同一二进制|
|Rust 工具源码变化|1.611 秒|2.182 秒|新键、新二进制，保留旧产物|
|两个并发冷请求|9.852 / 9.855 秒|10.307 / 10.307 秒|只编译一次，一次命中等待结果；两个摘要相同|

`legacy-fixture-comparison.json` 复现原 fixture 的独立 Cargo target 构建方式。
首次完整进程 8.312 秒，同一 target 再次调用 0.461 秒；新方式命中完整进程
0.321 秒。因此此次隔离 fixture 首次调用节省 7.991 秒，旧 target 已热时节省
0.140 秒。新工具首次建立缓存需 9.279 秒；这些数据只比较工具启动，不代表整条
PR 或发布流水线节省。没有旧版完整成功流水线的可比计时，不作外推。

验证结果：

- `unit-tests.log`：8 项缓存回归全部通过，13.837 秒；覆盖命中、源码/锁/参数/配置/
  编译器失效、二进制篡改、同键并发、不同参数隔离、编译中输入变化拒绝发布、
  验证包装器和不存在的相对 PATH 目录。
- `git-hooks.log`：16 项通过、4 项失败，原因都是暂存验证仍调用旧 example。
  不降低断言、不跳过失败；由协调者完成该入口替换后统一验证。
- `git-hooks-combined-probe.json`、`.log`：当前候选编译的 Rust 测试程序，运行时脚本
  放在独立诊断副本，只替换上述暂存入口；原有 20 项全部通过，完整进程 46.800 秒。
  此结果验证集成接口，不声称当前工作树完整套件已通过。
- `complete-candidate-proof.json`：隔离业务样例当前源码先通过，修改后在工具继续命中
  时 Cargo 测试返回 101；加入实际根 build.rs 的违规样例也返回 101。
  证明固定工具缓存不替代当前候选的业务测试或构建门禁。
- `package-tests.log`：原有源码包可复现和 build.rs 门禁两项测试通过，3.160 秒。
- `complete-source-package-proof.json`：源码包包含独立清单、依赖锁和新缓存入口；
  缺少这些文件会直接拒绝源码打包。

缓存身份覆盖所有 scripts Rust 源码、入口和缓存实现、独立 Cargo manifest/lock、
实际 Cargo/Rustc 二进制与版本、Rust host、平台、相关 Cargo 配置与 toolchain 文件、
编译器/链接工具、影响行为的环境和构建参数。业务 CARGO_TARGET_DIR 不参与工具身份，
工具构建显式使用自己的目录，业务 target 所有权规则没有改动。
构建条件不同使用不同目录；同条件编译只锁构建过程，完成后原子发布并验证二进制摘要。
损坏的缓存拒绝运行，不降级为未验证旧工具。

限制：仅预编译固定检查器；业务测试和原有 Rust 测试驱动继续由现有 Cargo 对当前候选
构建和调度。保留根 example 的测试覆盖，不搬运旧业务测试二进制。缓存首次构建依赖
已准备好的 Cargo 离线依赖；损坏记录需要显式清理该条缓存。没有修改日用插件、
客户端、共享 STATE、bindings 或 rust_adapter 的两线程配置。版本保持 0.3.97。
