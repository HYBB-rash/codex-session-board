# 连接、配置、服务与导航测试裁定

T0 登记的 22 个原 ID 中，4 项并入已有入口，9 项调整断言或触发条件，9 项保留原有检查。T2 为 2 项合并、3 项保留。T3 为 2 项合并、9 项修改、6 项保留。基线为 `f15754129ac64a1dd02ec93abb00f9237bf0c9f8`。

## T2 合并候选

| 原测试 ID | 处理及场景 | 出处 |
| --- | --- | --- |
| `lib::codex_session_board::rpc::tests::roles_reject_all_unrelated_methods_before_io` | 并入 `lib::codex_session_board::rpc::tests::each_role_accepts_exactly_its_three_declared_methods`。读连接、标题连接各自先拒绝六种越权请求，再用同一连接发送三种合法请求。对端核对收到的方法及顺序。 | `acceptance/requirements.json:262` |
| `lib::codex_session_board::rpc::tests::title_deadline_expires_during_websocket_handshake` | 保留。服务端已接受 Unix 连接却迟迟不回复握手。对应的 `lib::codex_session_board::rpc::tests::title_deadline_bounds_a_full_unix_listener_queue` 故意填满监听队列，连接仍在等待接入。任一阶段漏用总预算都会拖住标题同步。 | `acceptance/requirements.json:136` |
| `lib::codex_session_board::rpc::unavailable::tests::refused_desktop_socket_has_the_same_recovery_without_hiding_security_errors` | 保留。旧 socket 仍在，但监听者已退出。连接须给出重启指引并保留 `ECONNREFUSED`；把同一端点改为公开可写则须报告权限错误。`lib::codex_session_board::rpc::unavailable::tests::missing_desktop_socket_explains_recovery_and_preserves_the_cause` 独立检查五种角色遇到路径缺失或悬空链接的 `ENOENT`。 | `context/tasks/active/socket-recovery-guidance-20261111.md:12` |
| `lib::codex_session_board::service::migration::tests::aliases::migration_accepts_default_configuration_parent_alias_after_cli_resolution` | 并入 `lib::codex_session_board::service::migration::tests::aliases::migration_accepts_source_and_destination_root_aliases`。单次迁移从源目录别名读取，发布到目标目录别名；配置父目录也使用别名。解析前后认出同一文件，原别名可读新配置，原数据和链接仍在。 | `Docs/MIGRATION.md:33`，`Docs/REMOTE-SESSIONS.md:87` |
| `lib::codex_session_board::service::migration::tests::aliases::source_loading_accepts_configuration_and_data_aliases` | 保留。用户以真实路径、配置文件别名或父目录别名加载源配置，分别解析三种相对数据路径，含 `..`，再实际迁移。`lib::codex_session_board::service::migration::tests::aliases::migration_accepts_source_and_destination_root_aliases` 从已知数据库路径发布，无法发现源配置加载时选错库。 | `Docs/MIGRATION.md:33`，`Docs/REMOTE-SESSIONS.md:87` |

## T3 随业务替换断言

| 原测试 ID | 处理及场景 | 出处 |
| --- | --- | --- |
| `acceptance.test_native_new.NativeNewTests.test_native_navigation_and_assignment_do_not_create_or_send_input` | 保留浏览器交互，去掉扫描三份 Rust 源文件的字符串断言。`lib::codex_session_board::app::target_drafts::tests::canonical_target_request_uses_verified_rpc_and_only_opens_native_input` 在原生导航时观察到 `project/read`，未观察到创建会话、发送轮次或恢复会话。`lib::codex_session_board::app::project_assignment::tests::assignment_reads_writes_and_confirms_only_the_selected_uuid` 核对补登记的实际方法序列、UUID 和项目。 | `Docs/REMOTE-NATIVE-NEW.md:34`，`Docs/REMOTE-NATIVE-NEW.md:51` |
| `lib::codex_session_board::rpc::framing::tests::nonblocking_fixture_accept_waits_for_the_connection_and_has_a_deadline` | 移除夹具 `accept` 单测。`lib::codex_session_board::rpc::tests::stdio_handshake_filters_noise_and_reaps_child_on_drop` 在历史读取请求后延迟 20 ms 回包，仍收到列表；`lib::codex_session_board::rpc::tests::stdio_timeout_bounds_the_request_and_reader_shutdown` 让对端持续不回包，观察到 `TimedOut` 并及时退出。两个入口都实际经过 `deadline_io`。 | `Docs/REMOTE-PERFORMANCE.md:39`，`acceptance/requirements.json:167` |
| `lib::codex_session_board::rpc::login::tests::shell_quote_preserves_metacharacters_quotes_newlines_and_unicode` | 修改。把每个输入通过实际登录包装器和 `sh` 执行，核对命令收到的原字节。保留引号、换行、Unicode、空值和 NUL 拒绝。 | `context/tasks/completed/remote-connection-identity-worker.md:14` |
| `lib::codex_session_board::rpc::login::tests::wrapper_fails_closed_without_executable_login_shell_and_keeps_nonce_out_of_plain_command` | 改为 `lib::codex_session_board::rpc::login::tests::wrapper_requires_executable_login_shell_and_emits_the_requested_startup_marker`。无可执行登录 shell 时拒绝启动；两次传入不同标记，核对真实输出中的对应标记、合法 peer 及就绪回执。旧断言只比较两段命令文本有无差异。 | `Docs/REMOTE-SESSIONS.md:60`，`context/tasks/completed/remote-connection-identity-worker.md:14` |
| `lib::codex_session_board::rpc::runtime::tests::clock_four_second_limit_and_half_rtt_uncertainty_remain_exact` | 修改。验收读回投影后的远程完成时间；四秒往返下按最早可能值投影为 100，超预算或墙钟跳变则拒绝。四秒是项目既有安全界，产品文档要求有界采样，未规定这个数值。 | `Docs/REMOTE-SESSIONS.md:94`，`context/tasks/completed/remote-connection-identity-worker.md:16` |
| `lib::codex_session_board::rpc::tests::request_errors_are_downcastable_and_connection_remains_usable` | 保留。服务端对读取返回结构化错误，调用方仍要能取得原方法和代码，随后同一连接可继续获取会话列表。项目明确要求具体错误留在 anyhow 链中。 | `Docs/RELIABILITY-IMPLEMENTATION-PLAN-20261114.md:26` |
| `lib::codex_session_board::rpc::tests::sidebar_frames_wrap_caller_and_preserve_target_and_decode_content` | 保留。侧栏移动由调用者发起，对端收到目标会话，客户端读回成功。四字节小端长度、JSON、工具名和两个不同会话身份都跨越了客户端协议边界。 | `Docs/STATUS-SECTIONS.md:18`，`Docs/STATUS-SECTIONS.md:24`，`Docs/STATUS-SECTIONS.md:28` |
| `lib::codex_session_board::rpc::tests::stdio_handshake_filters_noise_and_reaps_child_on_drop` | 修改。历史读取需要跨过启动杂讯、通知和无关 ID，接收延时时间在预算内的目标回复，并在退出时回收子进程；延迟场景承接原夹具测试。对任意文本噪声的容忍只作为本次握手的输入。 | `acceptance/requirements.json:262`，`Docs/REMOTE-SESSIONS.md:61` |
| `lib::codex_session_board::rpc::tests::stdio_timeout_is_bounded_without_a_reader_thread` | 改为 `lib::codex_session_board::rpc::tests::stdio_timeout_bounds_the_request_and_reader_shutdown`。历史子进程接单后不回复，读取须返回超时且连同释放在原有 500 ms 上限内结束。保留该输入与错误类别，不限制线程实现。 | `acceptance/requirements.json:167`，`Docs/REMOTE-SESSIONS.md:61` |
| `lib::codex_session_board::rpc::tests::transport_failure_is_not_a_request_error` | 保留。对端关闭连接后，调用方必须能区别传输故障与某个会话的业务拒绝。与能继续复用连接的结构化拒绝输入分别验证。 | `Docs/RELIABILITY-IMPLEMENTATION-PLAN-20261114.md:26` |
| `example::remote_parity_lab::host_isolation_tests::host_re_registration_requires_explicit_migration` | 保留。独立实验模型给旧 hostId 的会话保存备注，再给新 hostId 发现相同 UUID，断言新主机没有继承，旧主机保留原备注。生产缓存回归 `lib::codex_session_board::remote::cache::tests::cold_cache_restores_removed_hosts_latest_notes_and_archive_tombstones_without_ssh` 会先给两个主机分别写入备注，不能独立证明第二个主机初始为空。此项证据限于实验模型。 | `Docs/REMOTE-PARITY-LAB.md:111` |
| `lib::codex_session_board::config::tests::initialization_never_overwrites_and_uses_private_permissions` | 修改。重复初始化保留原配置，拥有者可读写，组和其他用户没有权限。撤掉对拥有者执行位的固定要求。 | `Docs/MIGRATION.md:36`，`Docs/PARALLEL-DEVELOPMENT.md:69` |
| `lib::codex_session_board::config::tests::invalid_types_and_zero_quotas_are_rejected_before_creating_data` | 修改。11 种无效输入继续拒绝。原测试检查临时目录 `data`，但大多数输入实际指向用户的 XDG 默认数据目录。现在给可指定目录的输入明确设置 `data` 并在每轮检查副作用。空数据路径和非对象输入继续独立验证。 | `context/tasks/completed/package-authority-20261114.md:44`，`Docs/AUTO-ARCHIVING.md:4` |
| `lib::codex_session_board::service::tests::data_lock_serializes_independent_open_descriptors` | 并入 `lib::codex_session_board::service::tests::live_inaccessible_process_cannot_be_launched_again`。同一数据目录被另一拥有者锁定且 HTTP 失联时拒绝另起服务；释放旧监听器和拥有者锁后，由新打开的描述符进入允许启动的状态。 | `Docs/PLUGIN-RUNTIME.md:19` |
| `lib::codex_session_board::service::tests::proc_parser_handles_parentheses_and_zombies` | 保留。Linux 的 `/proc` 记录中，进程名含 `)` 仍要取得正确启动时间，僵尸返回已结束，坏记录报错。真正的 PID 复用检查无法稳定制造这三类内核输入。macOS 使用另外的进程身份入口。 | `acceptance/requirements.json:248`，`src/service.rs:102`，`src/service/macos.rs:9` |
| `lib::codex_session_board::workers::tests::join_reports_panics_and_errors_after_joining_every_handle` | 保留。一个线程 panic、另一个返回错误、第三个延迟完成。退出后调用方获知两种失败，第三个也已退出，重复等待仍报告失败。现有断言只辨认两种诊断，没有锁定完整报错格式。 | `Docs/PLUGIN-RUNTIME.md:24`，实施提交 `0abab9f` |
| `lib::codex_session_board::workers::tests::poisoned_registration_is_reported_and_existing_handles_are_joined` | 修改。登记锁损坏后不能再接任务，退出时既有线程必须完成，调用方收到失败。去除锁错误中文措辞匹配。 | `Docs/PLUGIN-RUNTIME.md:24`，实施提交 `0abab9f` |

## 回归证据

临时故障均在本机隔离副本触发，逐项核对实际失败，再还原源码。七项证据为：

| 注入情况 | 测试实际结果 |
| --- | --- |
| 拒绝历史读连接本有权执行的 `thread/list` | 合并后的角色测试报该调用不允许。 |
| 配置发布时不解析父目录别名 | 合并后的迁移测试报 `profile-alias` 是符号链接。 |
| 模拟对端额外记录一次 `thread/resume` | 原生导航测试报告该方法。夹具只记录事件。 |
| 将远程完成时间投影为中点 | 收到 102，要求 100。 |
| 将历史读取超时报成断管 | 实际 `BrokenPipe`，要求 `TimedOut`。 |
| 旧拥有者释放后仍拒绝启动 | 服务接管测试在释放后的第二次启动判断失败。 |
| 无效配置先创建显式指定的数据目录 | 第一条 `poll_seconds=0` 即在目录副作用断言失败。 |

Fedora 隔离回归通过 37 项库测试、1 项远程实验及 1 项 Chromium 交互；修正配置夹具后又复跑 2 项库测试。原绑定和结果绑定的差集为 6 个 ID（4 次合并、2 次改名），每个承接入口均在原有的每条需求绑定内。macOS 运行及真实客户端验收由发布阶段另行验证。
