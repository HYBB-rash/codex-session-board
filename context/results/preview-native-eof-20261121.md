# Fedora 真实预览入口的 EOF

`eb9b179470af72d1058fa4eb0fe9e61eb05a32eb` 的自动验收为 933/935。`native_authority` 的 50 项中只有 `entrypoints::preview_tool_keeps_reply_out_of_model_content` 失败，报告只保留了 `EOF while parsing a value at line 1 column 0`。同一源码随后按原来的两个测试线程重跑，50/50 通过；另在四路并发下定向跑了 12 次，均通过。这些复测不能还原首次失败的子进程状态。

**已证实的诊断缺口。** 原测试启动真实 `serve` 后从 stdout 读第一行。子进程在输出回执前退出时，`read_line` 成功返回零字节，夹具仍把它送去解析 JSON，并丢弃已接入管道的 stderr。原测试在只给该进程树设置相对 `XDG_DATA_HOME` 时得到了相同 EOF；回溯落在 `Live::new` 的第 59 行，尚未读取 Markdown。另一项归档重开测试的自然失败也留下了指向同一行的回溯。

| 真实入口对照 | 观察 |
| --- | --- |
| 原测试，在临时副本占住自己的夹具端口 | `serve` 退出 1，stdout 为 0 字节。子进程错误是 `Address already in use`，原测试只显示 JSON EOF。 |
| 诊断改动后，分别占住端口、给子进程树设置相对 `XDG_DATA_HOME` | 两次都报告 `serve`、端口、PID、退出 1、stdout 为 0 字节。stderr 分别指出端口占用和数据目录须为绝对路径。 |
| 在临时副本将首次预览工具请求改成无 ID 通知 | 真实 MCP 正常退出且没有回复；诊断标明 `stdio tools/call board_preview_reply`、退出 0 和 stdout 为 0 字节。 |

原报告缺少阶段、端口和子进程错误，无法把那一次失败追溯为端口占用、配置错误或另一个具体环境条件。共享启动夹具现在保留启动阶段的结果；HTTP 预览解码和 stdio 工具回执也分别标记来源。测试仍要求网页和真实 MCP 面板读到同一份完整回复，并保持内容对模型不可见。

独立复核另外使用真实子进程验证了坏 JSON、无效 UTF-8、退出前无输出、关闭 stdout 后继续运行，以及只写半行并保持管道打开。半行写端持续 36 秒；读取端在 30.26 秒返回超时，并列出 PID、存活状态、已读 11 字节和 stderr。正常启动立即读回；错误路径均保留原失败。

**同轮 owner 失败。** `plugin_runtime::changed_data_directory_cannot_create_another_owner_for_the_same_entry` 得到了错误回执，但其中没有断言期待的“正式实例仍存活”。原报告没有保存实际返回的错误文字。验收器先完成该测试组，再运行 `native_authority`，两者使用独立配置和进程。在单独对照中，占用后继配置的端口会使 owner 测试读到“端口上的服务属于其他看板或数据目录”；正常对照得到预期拒绝。不能据此认定两项历史失败同源。
