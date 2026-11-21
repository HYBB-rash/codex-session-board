# T2 工具维护测试配对结果

**11 行候选中，6 行合并，5 行保留独立测试。原版 21 个测试和新版 15 个测试分别在干净实现上全部通过。**

工作分支为 `codex/refactor-t2-maintenance-pairs`，基点为 `0ab5b20e797a352fa16a82fa93cc23c6215e0b1c`。认领提交为 `a7fe2d5`，测试和绑定提交为 `ed5801ebfed2c126416e6d9a9732dd5c57650486`。本页随后的记录提交只增加本页。最终提交、签名、逐提交规模及文件清单见[分支回执](</home/herman/Documents/Codex/2026-11-21/wo/work/t3-maintenance/t2-proofs/final-receipt.json>)。

## 逐项处置

每个 ID 由组前缀和表内的方法名组成。原始对照名称及文件行号见[十一组认领卡](../tasks/active/t2-maintenance-pairs-20261121.md)，原始判断见 [T0 账本](</home/herman/Documents/Codex/2026-11-21/wo/work/t0-b859880/ledger.json>)。末列编号对应[逐次故障结果](</home/herman/Documents/Codex/2026-11-21/wo/work/t3-maintenance/t2-proofs/fault-results.json> "sha256=83524e16c8ed301b1af35b35682cdf4f79e8cd8cfe8183cb8dcaf06a97b48a4c")中的运行次序，从 1 开始。每次记录给出原测试、后继测试、故障、退出码及两份日志。

| 组与完整前缀 | 原定义及规则来源 |
| --- | --- |
| P `acceptance.test_plugin_update.PluginUpdateTests.` | `53e7c56`，`acceptance/test_plugin_update.py`，`Docs/DEPLOYMENT.md:87`。归属 SERVICE-001。 |
| C `example::project_check::comment_policy::tests::` | `cf11cb5`，`scripts/comment_policy_tests.rs`，`Docs/CHECKS.md:85`、86、93。归属 GATE-001。 |
| S `example::project_check::commit_size::integration_tests::` | `aee5d00`，`scripts/commit_size_integration_tests.rs`，`Docs/CHECKS.md:126`、132。归属 GATE-001。 |
| D `example::project_check::document_policy::tests::` | `39169af`，`scripts/document_policy_tests.rs`，`Docs/CHECKS.md:52`、57、59。归属 GATE-001。 |
| H `test::git_hooks::` | `cf11cb5`，`tests/git_hooks.rs`，`Docs/CHECKS.md:33`、93、117。归属 GATE-001。 |

| 组、候选 ID 方法名 | 处置与后继 | 保留的输入、可观察结果与故障证据 |
| --- | --- | --- |
| P `test_worker_records_install_when_reload_connection_closes` | 保留原 ID；超时对照 `test_worker_records_install_when_reload_connection_times_out` 也保留。 | 两个方法共用断言，各次 `setUp` 准备新的临时目录，测试运行时取得安装锁、登记市场并写入回执。分别注入连接关闭及超时，检查末次 reload RPC、连接关闭、安装成功但待重载、清单、错误类型及原文、状态文件、pending 与 worker 快照清理。复用一次安装会改变第二种异常的前置状态。逐类丢类型或丢文本各令对应测试失败。001–004。 |
| C `copying_to_another_file_does_not_inherit_its_comment_allowance` | 保留原 ID；同文件对照继续由 `added_modified_and_duplicated_comments_fail_exact_multiset_comparison` 承接。 | 跨路径 fixture 先保留 `original.rs` 并复制到 `copy.rs`，再删除原件。两次都须拒绝。同文件重数表无法区分复制和移动。只在原件存在或只在原件消失时转移额度，分别触发第一或第二个断言。005–006。 |
| C `duplicate_baseline_counts_allow_only_the_recorded_number` | 合并至 `added_modified_and_duplicated_comments_fail_exact_multiset_comparison`。 | 两个独立基线。单份 `// old` 分别追加新注释、改字、仅增空格、复制旧注释，四次都须拒绝。双份 `// same` 单独验证减到一份、保留两份、增到三份，依次允许、允许、拒绝。对应故障依次误放新增、忽略字母或空格、多算单份额度、强求两份、放过第三份、把两份额度压成一份。007–013。 |
| C `nested_blocks_are_single_comments_and_doc_blocks_stay_documentation` | 合并至 `documentation_and_ordinary_comment_boundaries_follow_rust_tokens`。 | 两段原输入分别解析。四种近似文档的普通注释须在 5、6、7、8 行精确出现；四种文档标志均排除。另一段须在第 1 行返回完整嵌套普通块，在第 5 行返回 `// 末尾`，嵌套文档块排除。逐项测试误报、漏报、截断、错行或改末行正文。014–032。 |
| C `raw_identifiers_and_raw_lifetimes_are_not_lexical_errors` | 合并至 `literals_lifetimes_and_character_delimiters_are_not_comments`。 | 原始标识符、原始生命周期和原多行语料各调用一次解析器，每次均须成功而且结果为空。多行语料保留普通、转义、raw、byte、C 字符串、字符、转义引号、字节字符及普通生命周期。分别拒绝两种合法 raw token，或将 11 个具体字面量及生命周期 token 各误报一次。033–045。 |
| S `empty_files_count_toward_the_13_file_limit` | 保留原 ID 和 `executable_permission_changes_count_even_without_line_changes`。 | 同一新增序列在第 13、14 个空文件分别暂存，核对 `(13,0,0)` 允许、`(14,0,0)` 拒绝。权限用例先提交 0644 文件，再改 0755，核对 `(1,0,0)`。前者检查限额，后者检查 Git 能否统计纯模式变化。错设上限为 12 或 14、遗漏新增、遗漏模式变化，各触发相应断言。046–049。 |
| D `extensions_ignore_case_but_root_allowance_requires_exact_case` | 保留此候选 ID，承接 `exact_root_document_names_are_allowed`。 | 同一目录先仅放 `AGENTS.md`、`README.md`，核对零违规且 `check=true`。再加入 19 个大写扩展，逐路径核对完整列表。`agents.md`、`AGENTS.MD`、`readme.md`、`README.MD`、`Readme.md` 始终各用独立目录并核对唯一命中。故障逐个拒绝精确名称、拒绝干净目录、放过变体、漏报大写扩展或等量换错路径。050–057、058 至 094 的偶数、096。 |
| D `license_and_notice_are_not_exempt_from_document_extensions` | 合并至 `all_specified_document_extensions_are_reported`。 | 19 个 `guide` 小写扩展使用一个根目录，`LICENSE.md`、`NOTICE.txt`、`README.txt` 三个名称另用一个。各自检查排序后的完整路径和 `check=false`。局部故障逐个漏报名称，或在任一组仍有违规时错误放行。059 至 095 的奇数、097–101。 |
| H `documentation_and_comment_symbols_inside_strings_allow_a_commit` | 保留原 ID 和 `baseline_comments_survive_code_changes`，前者增加 HEAD 字节读回。 | 文档及字符串从空普通注释基线开始；另一仓库已分别准许普通行、块注释，新增 `next_value` 后仍须在 HEAD 中读到两处旧注释和新代码。两份仓库各自实际提交。故障拒绝带旧注释的代码编辑、逐处错分 5 段文档或 3 种字符串，以及检查成功后把索引恢复成旧源码。102–111。 |
| H `duplicating_a_baseline_comment_is_rejected` | 合并至 `ordinary_comments_block_commits_even_when_tests_and_clippy_pass`。 | 一个仓库以零份开始并新增普通注释，另一个以一份开始再复制同文。两者均在真实暂存后通过 Cargo 测试和 Clippy，再由真实提交钩子拒绝，保留 HEAD 和索引树。核对输出含注释及阻止原因，拒绝前未进入钩子的 Clippy 阶段。对基线多给一份额度、免费放过第一份两种故障分别导致错误提交。112–113。 |
| H `unstaged_ordinary_comments_do_not_reject_a_clean_staged_snapshot` | 保留原 ID 和 `unstaged_comment_removal_cannot_hide_a_staged_ordinary_comment`。 | 成功路径只改工作树，提交后核对工作树字节和 HEAD 无新注释。拒绝路径先把注释放进索引，再从工作树移除，核对注释诊断、HEAD、索引和工作树字节。检查器改读工作树后，两次真实提交在相反方向出错。114–115。 |

## 验证与复审

- 在基点导出完整源码，原版定向结果为检查器 13、钩子 6、Python worker 2；测试提交为 8、5、2。两份带故障开关的副本在关闭开关时也分别全过。完整 `project_check` 组 68 项通过，两个相关 Rust 目标通过 `cargo clippy --locked --offline --example project_check --test git_hooks -- -D warnings`。
- 两份副本使用[同一份故障补丁](</home/herman/Documents/Codex/2026-11-21/wo/work/t3-maintenance/t2-proofs/fault-new.patch>)，修改实际检查器、钩子或重载异常转换。每次只选择一个故障、运行一个精确测试名。115 组对照包括 113 种故障设置，110 组在原版与新版都失败。另 5 组原版通过、新版失败，分别覆盖双份额度原样保留、末行正文、违规路径、特殊名称目录的 `check` 返回值和成功提交的 HEAD 字节。[故障执行脚本](</home/herman/Documents/Codex/2026-11-21/wo/work/t3-maintenance/t2-proofs/run_faults.py>)
- 两版均实际编译并枚举 Linux 全部 Rust 测试，使用同一 Python 环境收集 unittest。基点 737 个 Rust 加 221 个 Python，后继 731 加 221，各自与验收绑定集合完全相等。逐需求比对只移除表中 6 个 GATE-001 重复入口；SERVICE-001 及其余绑定逐项一致。31 条需求、模型及锁与基点逐字相同。[原版清单](</home/herman/Documents/Codex/2026-11-21/wo/work/t3-maintenance/t2-proofs/original-inventory.json>)、[新版清单](</home/herman/Documents/Codex/2026-11-21/wo/work/t3-maintenance/t2-proofs/new-inventory.json>)
- 每次提交前运行 `./scripts/check --pre-commit`；测试提交的 `./scripts/check` 及 `sh scripts/verify_context.sh` 均通过。本机使用已配置的 `git commit -S`，以 Git 配置中的公钥独立验签。最终记录提交的复跑见[工作区检查日志](</home/herman/Documents/Codex/2026-11-21/wo/work/t3-maintenance/t2-proofs/final-check.log>)、[检查器定向日志](</home/herman/Documents/Codex/2026-11-21/wo/work/t3-maintenance/t2-proofs/final-example__project_check.log>)、[提交钩子定向日志](</home/herman/Documents/Codex/2026-11-21/wo/work/t3-maintenance/t2-proofs/final-test__git_hooks.log>)和 [worker 定向日志](</home/herman/Documents/Codex/2026-11-21/wo/work/t3-maintenance/t2-proofs/final-acceptance.log>)。

本次交付范围为 Linux 隔离测试和本地签名分支，组合提交由协调者独立复审。
