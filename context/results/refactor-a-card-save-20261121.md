# A 保存一张卡片

## 交付结果

**保存回执和随后快照共用后端卡片投影，页面直接采用确认结果。**  
版本、列、备注、新输入提示及原生位置不再由页面推算。  
普通拖放省略备注，编辑框仍明确提交字符串；失败草稿、临时落位和等待提示保持。

- 作者分支：`codex/refactor-a-card-save`
- 接单基线：`b859880165908a848a4982d2cfd9b722e9aeb912`
- 最终交付基线：`3de9e0f00f5458eda255b59e81cc54e63818f071`，包含 B1 `dce3536b70459c2962fa225320ecb88a8cde7522`、C1、T2 和 C2。
- 业务及审阅修复提交：`58bee839592994c344de173ba3431008aab0115f`
- B1 接入后完整复验提交：`d593d51c1c6f0cbfe89cb61727758b0bcf01db70`，仅比业务提交多 context 记录。
- 版本保持 `0.4.19`。后续收尾提交只修改本任务 context 记录。
- 实现指纹：`6391ae40b3ad8d07d587cc5059d5fb9b1d8b609c484d51210001fa8b2886454b`
- 交付范围：独立作者分支。等待 Herman 点名，不推进 main、推送、安装或部署。

`App::save_receipt` 直接从 `App::snapshot` 选取确认卡片，沿用 B1 合流后的 `store::placement::project`。  
受管远程保存返回根看板投影后的复合 ID、主机和项目身份。  
省略备注、归一化、版本和原生操作保护继续由原有后端实现负责。  
缺失或错身份回执保持待核对，读取快照确认，不重发保存。

## 首件的四份原值

| 来源 | 真实请求 note | 回执卡片 note | 刷新前卡片 note | 最终读回 note |
| --- | --- | --- | --- | --- |
| 未修复基线 | `"  "` | 未提供卡片 | `"  "` | `""` |
| 审阅修复后源码 | `"  "` | `""` | `""` | `""` |

最后一轮先将真实 fixture 卡片放到待审阅，再实际拖进已阻塞并输入两个空格。  
请求原生位置为待审阅，回执位置为已阻塞；回执、刷新前卡片和最终读回的版本一致。  
未修复和最终证据分别为 `refactor-a-first-baseline.json`、`refactor-a-b1-first.json`。  
后者绑定上述完整复验提交及实现指纹，包含浏览器进程组退出和 profile 删除回执。

## 测试服务的对象

下列是本任务认领或明确复用的行为测试，不引入新 Rust 测试 ID，不修改测试清单机制。

| 完整测试 ID / 场景入口 | 服务谁 | 输入 | 依赖并核对的行为 |
| --- | --- | --- | --- |
| `test::native_authority::entrypoints::built_binary_distinguishes_omitted_and_empty_notes_over_http_and_mcp` | 网页与共享 MCP 编辑者 | 省略、空串、旧备注、各类空白及旧版本 | 真二进制两个入口保存；回执各业务字段等于后续快照，旧写不改数据，重启保留 |
| `test::native_authority::entrypoints::built_binary_preserves_native_position_guards_over_http_and_mcp` | 同时在原生侧栏和看板操作的人 | HTTP/MCP 保存后原生位置外部改变 | 旧原生位置请求拒绝，不额外发移动 |
| `test::native_authority::remote_isolation::remote_online_disconnected_and_scanning_do_not_delay_local_http_mutations` | 多主机看板使用者 | mac/other 同 UUID、在线保存及离线/扫描期间本地保存 | 受管远程完整回执等于根快照，另一主机卡片不变，前台仍满足原 1000ms 门槛 |
| `test::remote_sessions::routes::remote_routes_bypass_local_authority_and_preserve_host_project_identity` | 兼容远程接口调用者 | 两主机同 UUID 分别保存 | 回执完整身份及卡片等于快照，不发本机标题写入 |
| `test::native_authority::transport::native_save_writes_remote_before_committing_notes_and_confirmation` | 原生保存使用者 | 目标列与备注 | 原生确认之前不提交本地备注 |
| `test::native_authority::transport::external_move_rejects_an_unobserved_stale_save_and_fresh_save_succeeds` | 并发编辑者 | 未被轮询观察到的外部位置改变 | 拒绝旧写；刷新后的新写可成功 |
| `test::native_authority::transport::lost_move_receipt_is_read_back_without_replaying_the_move` | 遇到原生丢回执的使用者 | 原生成功但回执丢失 | 读回实际位置且只发一次移动 |
| `test::native_authority::faults::input_arriving_during_a_native_move_cannot_be_confirmed_by_the_old_save` | 保存期间收到新输入的人 | 移动挂起期间注入用户输入 | 拒绝旧表单确认，保留原备注及新输入提示 |
| `test::board_contract::store_tests::test_block_allows_empty_reason_and_rejects_stale_edits` | Store 调用者 | 空白备注、过期状态版本 | 归一化及原子拒绝保持 |
| `test::board_contract::store_tests::test_new_input_during_drag_rejects_stale_confirmation` | Store 调用者 | 旧输入版本 | 不确认未核对的新输入 |
| `lib::codex_session_board::app::placement_tests::snapshot_places_local_and_remote_cards_from_work_evidence` | B1 合流后的根看板 | 本机、托管远程及三种列模式 | 历史、人工、运行、备注和空卡采用同一最终投影 |
| `lib::codex_session_board::app::placement_tests::snapshot_respects_native_positions_and_remote_presence` | B1 原生位置使用者 | 映射、默认、置顶、未知位置及远程离线 | 显示列、隐藏及未归列原因保持 |
| `lib::codex_session_board::app::placement_tests::snapshot_preserves_legacy_remote_cache_in_each_mode` | B1 旧缓存使用者 | 各列模式的根库远程缓存 | 缓存合流后的归列保持 |
| `lib::codex_session_board::catalog::view_tests::four_column_board_keeps_a_confirmed_empty_archived_chat_in_the_live_catalog` | B1 在线目录使用者 | 先活动再归档的同一空会话 | 已确认空的归档卡仍保留，目录元数据不伪造空证据 |
| `scripts/check-blocked-optional.mjs` | 实际网页编辑者 | 11 个取消、同列、备注和草稿场景 | 真实 Rust HTTP/数据库，记录 8 次请求、回执及刷新前卡片；失败空草稿可恢复 |
| `scripts/check-transfer-feedback.mjs` | 等待保存或切主机的人 | 69 个迟到、未知、冲突、草稿和未归列场景 | A 迟到不改 B 同 UUID；读回不重发；采用实际版本/列/新输入；保留可读完成提示 |
| `scripts/check-light-ui.mjs` | 四列及原有页面使用者 | 原 81 项和四列 21 项断言 | 控件、焦点、只读、布局及未归列保存仍有效 |
| `scripts/check-app-bridge.mjs` | MCP App 使用者 | 49 项路由及桥接断言 | SDK 的结构化回执和错误仍沿原入口传递 |

其他既有断言随相关测试目标一起运行，没有删除、跳过或降低要求。

## 实际验证

以下命令在上述完整复验提交上完成，全部退出 0。

```zsh
./scripts/dev setup
./scripts/dev start
node /private/tmp/refactor-a-first.mjs /private/tmp/refactor-a-b1-first.json --expect-fixed
node scripts/check-blocked-optional.mjs /private/tmp/refactor-a-b1-blocked.json
node scripts/check-transfer-feedback.mjs /private/tmp/refactor-a-b1-transfer
node scripts/check-light-ui.mjs
node scripts/check-app-bridge.mjs
./scripts/dev stop
./scripts/dev status
./scripts/check
NATIVE_LATENCY_REPORT=/private/tmp/refactor-a-b1-latency.json cargo test --locked --offline --test native_authority --test board_contract --test board_io --test remote_sessions -- --test-threads=1
cargo test --locked --offline --lib app::four_columns -- --test-threads=1
cargo test --locked --offline --lib app::placement_tests -- --test-threads=1
cargo test --locked --offline --lib catalog::view_tests -- --test-threads=1
cargo clippy --locked --offline --all-targets -- -D warnings
```

Rust 分组为 60、29、45、8、3、3 和 7 项，共 155 项，包含最新基线的 B1/C1/T2 变化。  
日志为 `refactor-a-b1-business.log`、`refactor-a-b1-four-columns.log`、`refactor-a-b1-placement.log`、`refactor-a-b1-catalog.log`、`refactor-a-b1-clippy.log` 和 `refactor-a-b1-check.log`。  
各开发检查点提交前均审阅暂存内容、执行 `./scripts/check --pre-commit` 并使用 `agent-git commit`。  
各开发提交分别为 5 文件 94 行、7 文件 137 行和 2 文件 18 行，保持原版本。  
收尾提交的准确哈希、相同实现指纹、context 校验、项目检查及 8 项直接相关测试由 `refactor-a-delivery.json` 绑定，独立审阅者再核对最终准确提交。

## 故障与独立审阅

- 基线两个空格在刷新前后不同，已在真实请求中复现，没有用页面模拟代替首件。
- 首轮受沙箱限制，fixture socket 返回 `Operation not permitted`；获得执行权限后才实际跑通，没有把它当作代码回归修复。
- 新增切主机场景会主动清空提示。前两轮浏览器回归在切换后读标题提示失败；现改为切换前保存并断言提示，原断言仍检查。
- 首轮全目标 Clippy 发现测试函数 42/41 行。将就绪和保存回执核对放入辅助函数后复验通过，未放宽限制。
- 独立审阅在 `2410bf7` 发现 P2，合法 `state:null` 回执会显示“已移到undefined”。新增实际浏览器断言在未修复代码上失败，修复提示后 69 场景通过。
- 独立续审准确提交 `6a83c5b3c6684256f57b7e2a936b78c009ea9a23`，无剩余阻塞项。原审阅与续审分别为 `refactor-a-review-code.json`、`refactor-a-review-fixed-code.json`。
- 首次 fetch 发现 main 前进后，已将 A 的四个提交 rebase 到 `808a0f5`，未发生源码冲突。普通 Git 的个人 1Password 签名失败后，使用 `agent-git` 完成滞留提交和 rebase，未修改个人签名配置。
- 独立复审 rebase 后准确提交 `3d3cd8ffeca1b91995f80fdb55bccd59cfe27eb6`，无业务阻塞项。C1/T2 的独占文件与基线 blob 一致，共享测试文件保留其模块声明。报告为 `refactor-a-review-rebased-code.json`。
- 接到 B1 合入通知后，再用 `agent-git rebase` 接入上述最新交付基线，仍无源码冲突。B1 的三类数据合流和最终归列没有第二套保存投影。
- 独立复审准确提交 `d593d51c1c6f0cbfe89cb61727758b0bcf01db70`，没有新增业务冲突或阻塞项；B1/C1/T2/C2 内容完整保留。源码复评为 `refactor-a-review-b1-code.json`，最终准确提交复评为 `refactor-a-review-final.json`。

原失败材料保留在 `refactor-a-transfer-first/`、`refactor-a-transfer-second/`、`refactor-a-unplaced-regression/` 和 `refactor-a-clippy-first.log`，不覆盖成通过结果。

## 证据位置与边界

源码外副本集中在[本聊天证据目录](/Users/hongyong/.codex/visualizations/2026/11/21/01a1fc5f-ab6b-7892-b2cf-ecef3dcdecc5/card-save-evidence)。  
最终 manifest 绑定收尾提交、业务提交、实现指纹、独立审阅及日志摘要。  
本任务开发目录 `/Users/hongyong/.csb-dev/0803d672b832` 已停止，`cleanup_confirmed=true`。  
独立 Chrome 均确认进程组退出并删除自己的 profile。

31 条需求、`requirements.json`、独立 `model.py`、`contract.lock.json` 和 `bindings.json` 相对交付基线均未修改。  
没有改归档批次、T0 清单机制、Vue 或列业务规则。  
这是 Mac 隔离 fixture、独立网页和共享 MCP 的开发验证，不代签日用 Codex 原生容器、两台现场主机、Linux 或完整 31 条目标环境验收。  
没有执行发布或宣称 `release_ready=true`。
