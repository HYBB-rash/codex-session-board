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
| `acceptance/test_remote_release.py` `test_duplicate_json_keys_and_changed_frozen_contract_are_rejected` | 歧义证明或变更冻结模型不能用于完整验收。 |
| `acceptance/test_rust_adapter.py` `test_only_verified_groups_receive_two_threads` | 运行验收采用已有验证的线程预算，其余目标保持串行。 |
| `acceptance/test_saved_release.py` `test_actions_token_checks_immutability_before_channel_writes` | 令牌无管理查询权限时，仍须读回不可变发布后再写渠道。 |
| 同文件 `test_draft_release_discovery_paginates_and_preserves_asset_digests` | 草稿位于后续列表页时找回原发布，后续读写保持同一 ID。 |
| 同文件 `test_first_release_checks_tag_reference_before_resolving_commit` | 第一次发包固定正确标签和源码，不能使用伪造的创建回执。 |
| 同文件 `test_local_then_ci_reuses_exact_archive_without_building` | 本地已上传包时 CI 重试复用同一归档字节。 |
| `acceptance/test_update_delivery.py` `test_native_add_can_only_clone_the_verified_local_snapshot` | 原生注册仅能克隆已经校验的本机快照。 |
| 同文件 `test_reload_does_not_accept_an_old_mcp_after_cache_update` | 缓存升级不代表旧宿主已启用新包，须读回正确版本与会话。 |
| `src/rpc/framing_tests.rs` `draft_factory_keeps_clock_validation_for_thread_start_and_thread_read_metadata` | 旧账本只读恢复拒绝新建，回读必须能核实会话时钟。 |

## 逐条处置

上表给出原用例、受益人和场景。下表同序，从 00 开始，以编号关联隔离对照日志。19 条改用可观察结果，4 条保留原断言。仅 03 改名，绑定同步；其余沿用原 ID。本任务未改生产程序。

| 编号 | 需求出处 | 处置与验证 |
| --- | --- | --- |
| 00 | `Docs/MAC-DEVELOPMENT.md:76` | 查系统进程表，验证该组已无存活进程。先给组员额外一次正常退出机会，再强制收尾能通过；假报清理成功但留下组员时失败。 |
| 01 | `Docs/VALIDATION.md:64` | 损坏清单从 CLI 验证错误退出、诊断非空且检查器未启动。错误解释换一种说法能通过；把损坏缓存当可执行条目时失败。保留五种非对象输入。 |
| 02 | `Docs/CHECKS.md:93`、`:100` | 保留。按文件、完整注释正文及次数与历史基线比较，再报告准确行号。CRLF、BOM 和多字节字符影响词法位置；在测试内统一删掉 CR 会掩盖正文比较变化。 |
| 03 | `Docs/CHECKS.md:51`、`:70` | 新 ID 为 `example::project_check::document_policy::tests::new_root_documents_are_all_reported`。核对违规文件全集，包括空格和换行。反序报告能通过，漏掉一个文件会失败。 |
| 04 | `Docs/CHECKS.md:79`、`:117` | 保留。解析器是 `scripts/check` 的实际 CLI；独立只读模式、暂存模式、准备版本和覆盖基线各有不同副作用。八个输入各自允许或拒绝一个真实命令。 |
| 05 | `Docs/DEPLOYMENT.md:15`、`:222` | 用包的实际平台和 CLI 的 JSON 输出核对单包及双包接口；稳定发布交给发布器的两份证据逐包匹配，证据不足提前拒绝。位置实参换成关键字实参通过；回执遗漏第二个平台或发布器收不到证据时失败。 |
| 06 | `Docs/INTEGRATION.md:37`、`context/tasks/completed/release-integrity-20261116.md:35` | 保留一次 `--atomic`、拒绝时两分支原状、重试和包字节检查。成功后将远端完整 ref 与发布回执逐项比对，替代统计 refspec 字符串。交换两 refspec 的顺序通过；只推一个平台、推完两个但回执漏一个分别失败。 |
| 07 | `Docs/MIGRATION.md:51` | 验证解包的构建门禁参与指纹、能运行、能阻止错误 MCP 入口及绕过连接层、门禁缺失时拒绝打包。等价的指纹 glob 和入口表达式换行均通过；禁用 MCP 入口守卫失败。 |
| 08 | `Docs/DEPLOYMENT.md:15` | 用不同清单代表两个平台，按解析后的分支区分并逐项比较。URL 中未转义斜杠通过；把两个平台都读到同一分支时失败。 |
| 09 | `Docs/INTEGRATION.md:45`、`context/tasks/completed/merge-release-split-20261116.md:63` | 同故障复用已知工单，仅允许读取；额外模拟读回发现来源标记已变。重复只读查询通过；跳过独立读回使用初始列表时失败。 |
| 10 | `Docs/DEPLOYMENT.md:161`、原生插件登记所需 `marketplace.json` 路径 | 目录误作文件时必须在打开桌面传输之前拒绝；放开错误英文原句。等价诊断通过；跳过文件检查会触发隔离传输哨兵而失败。 |
| 11 | `Docs/PLUGIN-RUNTIME.md:40`、`:47` | 按实际 RPC 请求身份响应。重载前确认当前线程，重载后读取该线程的分页库存，并向看板请求 `board_snapshot`；禁止再次发 `plugin/install`。额外读一次原线程通过；用 `board_view` 代替快照失败。 |
| 12 | `Docs/DEPLOYMENT.md:163` | 同设备号不保证可跨挂载交换，每次升级都须观察到市场操作且来源在模拟的目标挂载内。多一级暂存目录通过；改在缓存挂载暂存会遭 `EXDEV`。 |
| 13 | `Docs/PLUGIN-RUNTIME.md:46`、`Docs/DEPLOYMENT.md:125` | 升级开始前和即将重载时各有读者打开回执，持有的文件内容在后续发布后仍完整。最后重新打开能得到最新回执，原始安装信息也仍可读。额外原子写同一结果通过；中间或最终一步单独改为原地写，各自失败。 |
| 14 | `acceptance/README.md:68`、`:70`、`context/handoffs/remote-release-gate-worker.md:22` | 通过完整专项验收入口提交一份其他字段都有效、含重复版本键的清单，覆盖两个版本相同和冲突；改动冻结模型须报告该文件。修改解析诊断措辞通过；普通 JSON 的后值覆盖或跳过冻结锁各自失败。 |
| 15 | `Docs/MIGRATION.md:53`、`context/results/test-parallelism-20261116.md:49`、历史 `3986e4f` 的 `Docs/INTEGRATION.md:96` | 保留。历史规则明确钩子与检查器两组**每组**两线程；现行文档确认两组并行、库与原生组的内部线程预算和其他业务默认串行。既检查传给 Rust 测试程序的预算，也检查计时回执，二者均可被维护者观察。 |
| 16 | `Docs/DEPLOYMENT.md:33`、`:35` | 让真实发布 CLI 解析参数，检查到达渠道写入边界的每份包的平台；只有发布已冻结且非草稿才允许。增加本地只读检查、把 `--packages A B` 改为两次 `--package` 都能通过；放过可变发布失败。 |
| 17 | `Docs/DEPLOYMENT.md:23`、`:25`、`:30` | 分页找回第二页的草稿，后续读写固定到同一数字 ID；不同页出现两个匹配草稿时拒绝。还保留身份变化和访问失败。额外读同一 ID 或调整查询参数顺序通过；只读第一页、找到第一条匹配便停止，分别失败。 |
| 18 | `Docs/DEPLOYMENT.md:23`、`:26` | 保留新标签先建立再解析为正确提交、草稿绑定提交，以及布尔参数和字符串参数的区别。放开 GitHub CLI 中两个字段的排列顺序，交换字段通过；错误地把布尔值按字符串提交时失败。 |
| 19 | `Docs/DEPLOYMENT.md:25`、`:30` | 用已创建的 Release 数、核验后的包和内容未变化证明复用；不计 GET 次数。重复查询同一标签通过；已有有效包时再次调用构建失败。 |
| 20 | `Docs/DEPLOYMENT.md:94` | 对五种仓库输入，验证原生注册仍使用固定提交，Git 仅允许本地文件协议，并实际执行 Git URL 解析取得已验证目录。配置槽从 0 移至 1 通过；改指其他目录失败。控制使用 `--get-url`，不访问网络。 |
| 21 | `Docs/DEPLOYMENT.md:123`、`Docs/PLUGIN-RUNTIME.md:40` | 缓存升级后旧版本、缺少快照、原生线程未加载均拒绝。已拒绝的宿主上只允许握手与读取；额外只读原线程通过，提前发重载请求失败。 |
| 22 | `Docs/REMOTE-SESSIONS.md:39`、`:64`、`:95` | 保留。旧账本恢复收到新建方法时须在转发前拒绝；只读方法确实送往正确远端，但读取错误时钟时仍必须报错。两条路径分别验证零次转发与实际转发，删除任一都会丢失可观察边界。 |

## 证据与边界

本机使用独立源码副本、专用进程组、回环 fixture 和本地 Git。首次运行对照时，一次缺少测试模块的导入路径，一次因校验突变进入模拟 socket 等待超时。这两次均不计入故障检出；补齐环境、用禁止接触桌面传输的哨兵后重跑。每个变更的用例各有等价与故障两组控制。`controls.py` 保存每次临时源码改动，`case-NN-equivalent.log`、`case-NN-fault.log` 和 `controls-*.json` 保存退出码与输出摘要。

首轮审阅在 `1b0f1f5` 复现了仅收尾原地写回执、翻页遇首个草稿即停止仍被放行，以及几种合法调用被误拒。补入跨页冲突、两个独立读者、真实发布参数解析和按请求响应的桌面 fixture，并复测逐项记录中的额外反例。所有等价对照均直接运行已提交的测试。

证据目录为 `/home/herman/Documents/Codex/2026-11-21/wo/work/t3-maintenance/`。19 项 Python、3 项检查器测试和 1 项原生时钟测试在 Linux 定向核对。项目检查、冻结契约、绑定及独立审阅的最终提交身份见目录中的终验回执。主线、macOS、实机和发布继续由协调者核对。
