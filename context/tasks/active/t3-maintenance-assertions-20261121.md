# T3 交付与开发工具断言

- owner: python_scenarios
- execution_mode: isolated_write
- allowed_write_set: 下表认领的现有测试及本任务的唯一记录
- base_commit: b859880165908a848a4982d2cfd9b722e9aeb912
- worktree_path: /home/herman/.codex/worktrees/t3-maintenance-assertions/notion-program
- coordinator: 五轮重构协调会话

依据 T0 口径筛出 23 个 ID。来源见下表，各文件中的方法名称唯一。验收冻结需求、参考模型和锁保持独立；T1、T2 及其他业务作者的用例不在本任务范围。

| 文件及测试名称 | 受益人、触发与期望 |
| --- | --- |
| `acceptance/test_dev_environment.py` `test_owned_group_cleanup_terminates_a_child_that_ignores_interrupt` | 开发者停止顽固子进程，开发环境只清理自己的进程组。 |
| `acceptance/test_checker_cache.py` `test_non_object_cache_records_return_a_check_error` | 检查器使用者读到损坏清单，命令必须失败并提供诊断。 |
| `scripts/comment_policy_tests.rs` `bom_shebang_crlf_and_unicode_preserve_comment_lines` | 作者需要正确定位带 BOM、中文和 CRLF 的注释。 |
| `scripts/document_policy_tests.rs` `new_files_are_checked_without_a_git_repository_and_sorted_stably` | 作者应在根目录得到全部新增违规文件的清单。 |
| `scripts/project_check.rs` `comment_mode_excludes_preparation_and_controls_repository_override` | 检查模式不能顺便准备版本或任意改基线目录。 |
| `acceptance/test_delivery.py` `test_bulk_cli_preserves_legacy_single_package_and_requires_both_records` | 发布者从单包、多包命令提交完整参数，证据数量不一致即拒绝。 |
| `acceptance/test_delivery.py` `test_platform_channel_updates_are_atomic_and_retries_reuse_commits` | 发布两个平台必须一次原子变更渠道；重试复用原包和提交。 |
| `acceptance/test_gate.py` `test_package_requires_the_fingerprinted_build_gate_and_preserves_its_rejection` | 迁移使用者解包后，仍受与原项目相同的构建入口和连接门禁约束。 |
| `acceptance/test_main_release.py` `test_channel_manifest_reads_existing_candidate_branches` | 发布状态查询能各自读出真实平台分支的清单。 |
| `acceptance/test_main_release_feedback.py` `test_failure_reuses_one_source_bound_incident_and_reports_previous_channels` | 同一发布故障复用已有工单，携带故障来源及上次发布状态。 |
| `acceptance/test_plugin_update.py` `test_desktop_install_rejects_directory_instead_of_marketplace_file` | 安装入口误传目录时，不能请求原生安装或登记加载成功。 |
| 同文件 `test_full_update_reloads_existing_native_install_without_install_rpc` | 已安装时重载并读回正确插件与会话，不重复安装。 |
| 同文件 `test_market_staging_stays_on_its_mount_even_when_device_ids_match` | 相同设备号的独立挂载点仍要求同挂载暂存。 |
| 同文件 `test_repeated_upgrades_preserve_original_receipt_with_atomic_current_writes` | 重复升级保留最早回执，并以原子方式记录新成功。 |
| `acceptance/test_remote_release.py` `test_duplicate_json_keys_and_changed_frozen_contract_are_rejected` | 歧义证明或变更冻结模型不能售作已完整验收。 |
| `acceptance/test_rust_adapter.py` `test_only_verified_groups_receive_two_threads` | 运行验收采用已有验证的线程预算，其余目标保持串行。 |
| `acceptance/test_saved_release.py` `test_actions_token_checks_immutability_before_channel_writes` | 令牌无管理查询权限时，仍须读回不可变发布后再写渠道。 |
| 同文件 `test_draft_release_discovery_paginates_and_preserves_asset_digests` | 草稿位于后续列表页时找回原发布，后续读写保持同一 ID。 |
| 同文件 `test_first_release_checks_tag_reference_before_resolving_commit` | 第一次发包固定正确标签和源码，不能使用伪造的创建回执。 |
| 同文件 `test_local_then_ci_reuses_exact_archive_without_building` | 本地已上传包时 CI 重试复用同一归档字节。 |
| `acceptance/test_update_delivery.py` `test_native_add_can_only_clone_the_verified_local_snapshot` | 原生注册仅能克隆已经校验的本机快照。 |
| 同文件 `test_reload_does_not_accept_an_old_mcp_after_cache_update` | 缓存升级不代表旧宿主已启用新包，须读回正确版本与会话。 |
| `src/rpc/framing_tests.rs` `draft_factory_keeps_clock_validation_for_thread_start_and_thread_read_metadata` | 旧账本只读恢复拒绝新建，回读必须能核实会话时钟。 |
