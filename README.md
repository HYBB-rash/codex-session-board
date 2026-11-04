# Notion Program · Rust

这是原 Python 项目的 Rust 重写，代码位于 `/home/herman/文档/notion-program`。
原项目作为对照保留在 Documents 目录。Rust 使用独立配置；
本机切换时沿用原数据和任务目录，保持已有任务的检查点、聊天和文章归属连续。

程序负责读取 Notion 任务、按所选模型创建或继续执行聊天、跟踪结果、
提交审核状态，以及把已完成任务的知识归入项目上下文。
报告正文、轮次文章和审阅原文由执行聊天处理；调度器只处理任务属性和收据。

## 编译与验证

会话或开发任务结束前统一运行 `./scripts/check`。
当前检查工作区是否干净、每次提交的版本步进和测试钩子是否启用，失败时提供处理命令。
提交前使用 `./scripts/check --prepare-commit` 准备版本；
阶段完成升版须由用户明确决定。详细规则见 `CHECKS.md`。

首次使用或重新克隆仓库后，运行 `./scripts/install-git-hooks` 启用提交前测试。
之后每次提交都会对暂存区代码运行 `cargo test --locked --offline`，
测试失败或无法运行都会阻止提交。

```bash
cargo build --release --locked
cargo test --locked
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
```

可执行程序是 `target/release/notion-program`。运行时不需要 Python 或 Python 虚拟环境。
构建需要 Rust/Cargo，第一次构建会下载 Cargo.lock 中锁定的依赖。
本项目在 Linux / Rust 1.99 上验证；本地套接字和进程管理针对 Linux。

测试只使用临时目录、模拟客户端、本机测试套接字和本机 HTTP 服务，
不连接真实 Notion、不发送真实聊天消息。
`tests/fixtures/content-comparison.json` 保存了从原项目比较器提取的输入和预期结果，
Rust 测试直接读取这份数据，不需要 Python 源项目。

`examples/live_validation.rs` 是需要显式运行的线上验证工具，可以创建标记清楚的测试项目、
计划中任务及归档样本。创建前保存意图，结果不明时保留记录核对，不自动重复创建。
它独立于 `cargo test`；实际运行会写入 Notion，激活测试任务后会触发真实执行。

## 配置

可以从示例开始，填写自己的数据源 ID 和本地套接字：

```bash
cp config.example.json config.json
cp archive-config.example.json archive-config.json
```

也可以迁移原项目的配置：

```bash
target/release/notion-program import-config \
  --source /home/herman/Documents/Codex/2026-11-02/new-chat-2/work/notion-program \
  --destination /home/herman/文档/notion-program
```

配置迁移会保留模型、数据源和路由设置，将工作目录、任务目录和数据目录指向新项目，
并把 Python WebSocket 桥替换成 Rust 的原生套接字连接。已存在的配置不会被覆盖。
它只迁移配置，不复制任务状态，不启动服务，不改变旧定时器。

```bash
target/release/notion-program --config config.json check-config
target/release/notion-program --config archive-config.json check-config
```

相对路径以配置文件所在目录为基准。程序、配置和数据分别放置；
下表的数据位置是独立新部署的默认布局，实际位置由配置决定：

| 位置 | 用途 |
| --- | --- |
| `src/` | Rust 程序 |
| `config.json`、`archive-config.json` | 本机配置 |
| `data/state.json` | 调度检查点，保留原 JSON 字段和阶段名称 |
| `data/events.jsonl` | 追加式事件记录 |
| `tasks/<任务 UUID>/` | 每项任务的快照、执行意图、结果和诊断 |
| `data/archive-runtime/` | 归档检查点、写入意图、读回和收据 |
| `execution-templates/` | 执行聊天使用的正文模板 |
| `deploy/` | systemd 服务与定时器示例 |

`daemon_socket` 指向持久 App Server 的 Unix WebSocket 套接字。
也支持 `codex_command` 数组启动 JSON 行协议进程，方便其他部署方式与离线测试。
桌面聊天兼容路径使用 `desktop_tools_command` 和 `desktop_caller_thread_id`；
这些本机信息应放在配置里。
Notion MCP 使用已经连接的账号；独立 REST 客户端由调用者传入凭据。

## 常用命令

```bash
target/release/notion-program --help
target/release/notion-program --config config.json status
target/release/notion-program --config config.json probe
target/release/notion-program --config config.json inspect --page TASK_UUID
target/release/notion-program --config config.json inspect --page TASK_UUID --body --output task.json
target/release/notion-program --config config.json --trigger manual_test run
target/release/notion-program --config archive-config.json archive run
target/release/notion-program --config archive-config.json archive run --probe
target/release/notion-program --config archive-config.json archive context --task TASK_UUID --output context.json
```

`check-config` 和 `status` 只读本地文件。`probe` 和 `inspect` 只读远端，
建立工具上下文但不启动模型回合。`run` 和 `archive run` 会执行实际工作。
省略子命令时执行一次 `run`，兼容原入口的 `--config` 和 `--trigger` 用法。

项目上下文 helper 示例：

```json
{
  "project_context": {
    "enabled": true,
    "helper_command": [
      "/absolute/path/notion-program",
      "--config",
      "/absolute/path/archive-config.json",
      "archive"
    ]
  }
}
```

执行者会在这些参数后添加 `context --task ... --output ...`。
每轮冻结其配置，续接不会通过改配置跳过已有约束。

## 保留的工作规则

- 只有 `待办事项` 能开始新轮次；`未开始`、`计划中` 不派单。
- 创建聊天、发送意图、执行回合和状态写入分别保存检查点。
  观察到匹配的回合真正运行后才发布 `进行中`；已经完成的回合可直接提交审核。
- 发送或写入结果不明时先核对，进程重启和重复定时检查不直接重发。
- 人工续试与有限自动续试保留聊天、轮次、权限、快照和文章。
  等待审批或输入时不自动绕过。
- 已绑定的历史索引和轮次文章保持独占；完成收据检查任务、轮次、token、哈希及归属声明。
- 归档先保存来源、生成并校验方案，再写专题、上下文和收据，最后更新任务状态。
  不完整读取、来源变化或不确定写入保留检查点以便核对。

调度器验证的是执行者收据和已知路由；正文是否完整读回由执行者声明，
这不能代替 Herman 的审核。文件锁只保护同一数据目录里的程序，
Notion 的读取和属性写入仍是分开的操作。

## 与原程序交接

`import-config` 不自动接管旧的 `data/`、`tasks/` 或 systemd 服务。
切换正在运行的部署时，需要先停止旧入口，在锁保护下备份检查点和任务文件，
核对其中的绝对路径，再让新服务使用同一份连续状态。
不要让两份独立状态库同时调度同一个追踪表。

`deploy/` 提供本机 systemd 单元，其他主机应先调整路径。
本次经明确授权进行的服务切换、线上验证与回退方法见 `DEPLOYMENT.md`。
一次性 demo、历史事故修复和旧部署切换脚本中的固定任务 ID 不作为默认操作；
使用 `inspect`、本地状态和显式 `rpc` 命令按实际对象核对。
`rpc` 是运维入口，只有明确请求该 RPC 时才使用，可能产生远端写入。

模块对应关系和验证范围见 `MIGRATION.md`。
