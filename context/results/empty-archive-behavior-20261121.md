# 空归档名单的行为对照

基线 `77f282b8a055b6d2d793a99aee5784f9ba5509d8`；分支 `codex/empty-archive-behavior`。
实际入口为 `cargo test --locked --offline --test board_io archive_batch_tools_share_http_history -- --exact --nocapture`，保留 MCP-001 的原 ID 和有效批次目标字段、版本及状态断言。

服务从临时只读原生后台读到指定 UUID 后，测试用 stdio MCP 提交空名单，要求 `isError=true` 和非空原因。
MCP 与 HTTP 的批次历史均只含原有效名单，SQLite 中被拒的批次 ID 有零行。随即用同一个 ID 仅把空名单改成有效名单，创建成功。
只读后台在关闭所有连接时汇总处理结果，任何写方法都会使测试失败。原生夹具复用自 `tests/plugin_runtime/authority.rs`。

| 隔离对照 | 退出码 | 实际回执 |
| --- | ---: | --- |
| 基线测试，更换为“请至少选择一个会话再登记归档批次” | 101 | 拒绝标志正确，旧断言逐字比较失败 |
| 候选测试，原措辞及以上等义措辞 | 0、0 | 空名单拒绝，同 ID 有效名单成功 |
| 放行空名单 | 101 | MCP 回执缺少 `isError=true` |
| 空名单先入库再返回原错误 | 101 | 历史中多出第二个空批次 |
| 空名单触发原生归档后仍返回原错误 | 101 | 隔离后台实际收到 `thread/archive` |

原始记录和四组实现变体的 `.patch` 在作者工作树同级 `evidence/`。T0 旧版措辞对照见 `old-wording.log`。复审修复后在冲突端点下重复上述对照，见 `p2/base.log`、`p2/wording.log`、`p2/accepted.log`、`p2/persisted.log` 和 `p2/native-write.log`。

服务、MCP 和清理命令均显式使用本测试的临时 Unix 端点。侧栏目录使用私有空项目文件，数据库直接以只读模式打开。其余测试继续沿用各自的命令环境。
修复前，无效的继承端点使原测试退出 101；修复后同一条件通过。Linux inotify 监控模拟日用侧栏，在正常和强制超时两次运行都收到零事件。移除临时侧栏配置的独立对照记录到打开和读取。超时报告只含 `runtimeVerified=true, targetPresent=false`。原始输出和监控结果见 `p2/base.json`、`p2/timeout.log`、`p2/timeout.json`、`p2/missing-sidebar.json`。
`p2/test-blobs.txt` 和 `p2/staged-tree.txt` 分别绑定测试源码和暂存树。签署与合入由协调者的队列负责。
