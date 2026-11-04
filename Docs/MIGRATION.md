# Rust 移植说明

源项目：`/home/herman/Documents/Codex/2026-11-02/new-chat-2/work/notion-program`

目标项目：`/home/herman/文档/notion-program`

## 模块对应关系

| 原模块 | Rust 模块 |
| --- | --- |
| `scheduler.py` 中现行入口、持久化和事件 | `src/main.rs`、`src/common.rs`、`src/scheduler.rs` |
| `state_scheduler.py`、`dispatch_start.py` | `src/scheduler.rs` |
| `automatic_retry.py`、`user_retry.py` | `src/scheduler/retry.rs` |
| `status_feedback.py` | `src/scheduler/feedback.rs` |
| `project_context_gate.py` | `src/scheduler/context.rs` |
| `codex_client.py`、`daemon_bridge.py` | `src/clients/rpc.rs` |
| `desktop_executor.py` | `src/clients/routing.rs` |
| `notion_mcp.py` | `src/clients/notion.rs` |
| `notion_client.py` | `src/rest.rs` |
| `round_history.py` | `src/history.rs` |
| `delivery_integrity.py` | `src/integrity.rs` |
| `diagnostics.py` | `src/diagnostics.rs` |
| `archive_knowledge.py` | `src/archive/knowledge.rs` |
| `archive_runtime.py` | `src/archive/` |
| 只读连接与任务检查脚本 | `probe`、`inspect`、`status`、`rpc` 子命令 |
| systemd 单元 | `deploy/` |

现行入口对应原来的 `StateScheduler`。旧 `Scheduler` 中由调度器读取和追加任务正文的
历史工作流不作为新入口运行；正文完整性比较器保留为独立 Rust 接口。
原有一次性演示、事故修复、旧自动化切换脚本包含特定历史任务和路径，
在新工程中通过通用检查与运维入口取代，不会自动重放历史操作。

## 状态与文件

状态文件仍使用 JSON，事件仍使用 JSONL。阶段和已有字段保持可识别，
额外未知字段原样保留。新轮次使用 `confirmed_start_v1`、
`execution_chat_owns_page_v1` 和 `per_round_article_v2`。

任务结果、派发意图和归档写入意图分别存放。状态更新采用临时文件、同步写入和重命名；
新意图与已有不确定写入仍通过检查点核对。数据不自动从旧目录导入。

配置示例把机器路径与程序代码分开，`import-config` 可以迁移实际机器设置，
其中 Python bridge 被 Rust 原生 Unix WebSocket 连接替代。
外部 Codex 桌面工具仍属于配置的外部服务。

## 验证口径

移植开始前，原 Python 共运行 574 项离线测试，结果为 OK，1 项跳过。
这是对照基线，不代表 Rust 逐项执行了原来的 Python 测试。

Rust 测试覆盖状态机、重启恢复、写入核对、续试、收据、归档方案与安全写入、
协议解析、超时和本地传输，并使用固定输入验证正文比较行为与原实现一致。
两个测试集的数量不能直接等同。

当前 Rust 结果：121 项库测试、4 项 CLI 测试、1 项包含 72 组对照数据的差异测试，
共 126 项测试通过；格式检查和全部 target 的严格 Clippy 检查通过。

离线测试与线上验证分别记录。真实验证工具使用独立测试项目和带归属标记的样本，
不将自动化测试当作 Herman 人工审核通过。部署过程、线上证据和验证边界见 [DEPLOYMENT.md](DEPLOYMENT.md)。
