# T2 检查器、提交钩子及插件更新配对

- owner: T2 maintenance pairs
- execution_mode: isolated_write
- base_commit: 0ab5b20e797a352fa16a82fa93cc23c6215e0b1c
- worktree_path: /home/herman/.codex/worktrees/t2-maintenance-pairs/notion-program
- coordinator: 五轮重构协调会话
- allowed_write_set: 下表 11 个候选及其对照测试、这些 ID 在 `acceptance/bindings.json` 的绑定、本卡和 `context/results/t2-maintenance-pairs-20261121.md`

来源为 [T0 账本](</home/herman/Documents/Codex/2026-11-21/wo/work/t0-b859880/ledger.json>) 的 `live` 表。下表各行的候选与对照均使用第一列所指的完整前缀。C、S、D 分别为 `example::project_check::comment_policy::tests::`、`example::project_check::commit_size::integration_tests::`、`example::project_check::document_policy::tests::`，H 为 `test::git_hooks::`，P 为 `acceptance.test_plugin_update.PluginUpdateTests.`。

| 前缀 | 候选 | 对照 | 原定义位置（候选、对照） |
| --- | --- | --- | --- |
| P | `test_worker_records_install_when_reload_connection_closes` | `test_worker_records_install_when_reload_connection_times_out` | `acceptance/test_plugin_update.py` 311、315 |
| C | `copying_to_another_file_does_not_inherit_its_comment_allowance` | `added_modified_and_duplicated_comments_fail_exact_multiset_comparison` | `scripts/comment_policy_tests.rs` 221、179 |
| C | `duplicate_baseline_counts_allow_only_the_recorded_number` | `added_modified_and_duplicated_comments_fail_exact_multiset_comparison` | 同文件 198、179 |
| C | `nested_blocks_are_single_comments_and_doc_blocks_stay_documentation` | `documentation_and_ordinary_comment_boundaries_follow_rust_tokens` | 同文件 108、83 |
| C | `raw_identifiers_and_raw_lifetimes_are_not_lexical_errors` | `literals_lifetimes_and_character_delimiters_are_not_comments` | 同文件 101、64 |
| S | `empty_files_count_toward_the_13_file_limit` | `executable_permission_changes_count_even_without_line_changes` | `scripts/commit_size_integration_tests.rs` 143、202 |
| D | `extensions_ignore_case_but_root_allowance_requires_exact_case` | `exact_root_document_names_are_allowed` | `scripts/document_policy_tests.rs` 58、19 |
| D | `license_and_notice_are_not_exempt_from_document_extensions` | `all_specified_document_extensions_are_reported` | 同文件 183、28 |
| H | `documentation_and_comment_symbols_inside_strings_allow_a_commit` | `baseline_comments_survive_code_changes` | `tests/git_hooks.rs` 509、486 |
| H | `duplicating_a_baseline_comment_is_rejected` | `ordinary_comments_block_commits_even_when_tests_and_clippy_pass` | 同文件 560、462 |
| H | `unstaged_ordinary_comments_do_not_reject_a_clean_staged_snapshot` | `unstaged_comment_removal_cannot_hide_a_staged_ordinary_comment` | 同文件 544、529 |

P 归属 SERVICE-001，其余归属 GATE-001。按原提交运行全部认领项，逐对核对前置条件、断言、隔离边界与后继绑定。在源码外对照故障前后的定向结果，完成后以签名本地提交交协调者复审。
