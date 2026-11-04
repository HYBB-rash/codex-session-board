# Notion Program · Rust

## 项目简介

这是原 Python 项目的 Rust 重写，用于从 Notion 调度任务、按所选模型创建或继续执行聊天，
并将已完成任务的知识归入项目上下文。
调度器管理任务属性、路由和收据；报告正文、轮次文章及审阅原文由执行聊天处理。
原 Python 项目仅作只读对照，切换部署时须保持已有任务、聊天和文章的归属连续。

## 代码导航

| 位置 | 用途 |
| --- | --- |
| [src/main.rs](src/main.rs) | 命令行入口、本地配置校验与配置迁移 |
| [src/scheduler.rs](src/scheduler.rs)、[src/scheduler/](src/scheduler/) | 调度状态机、续试、项目上下文和诊断反馈 |
| [src/clients.rs](src/clients.rs)、[src/clients/](src/clients/) | Notion MCP、执行聊天路由及 App Server RPC 传输 |
| [src/archive/](src/archive/) | 知识归档、写入核对、项目上下文与通知 |
| [src/history.rs](src/history.rs)、[src/integrity.rs](src/integrity.rs) | 历史与轮次文章约定、交付收据及正文比较 |
| [src/common.rs](src/common.rs)、[src/rest.rs](src/rest.rs) | 配置与文件操作、独立 Notion REST 客户端 |
| [tests/](tests/) | CLI、Git 钩子和原 Python 比较器契约测试；单元测试位于源码模块中 |
| [execution-templates/](execution-templates/) | 执行聊天使用的任务主页、历史索引和轮次文章模板 |
| [deploy/](deploy/) | systemd 服务与定时器示例，使用前需核对本机路径 |
| [scripts/](scripts/)、[.githooks/](.githooks/) | 项目约束检查、钩子安装及提交前检查 |

## 开发与检查

需要 Git、Rust/Cargo、rustfmt 和 Clippy；现有套接字与进程管理面向 Linux，
项目已在 Rust 1.99 上验证。首次使用或重新克隆后，在项目根目录执行：

```bash
cargo fetch --locked
./scripts/install-git-hooks
```

首次下载依赖需要网络；后续验证使用锁文件中的离线依赖：

```bash
cargo test --locked --offline
cargo fmt --check
cargo clippy --all-targets --locked --offline -- -D warnings
./scripts/check
```

普通测试只使用临时目录、模拟客户端和本机测试服务，不连接真实 Notion 或 Codex。
开发时查看 CLI 可用 `cargo run --bin notion-program --locked --offline -- --help`。
本地检查和测试无需填写生产配置。首次配置及运行命令见[配置与命令](Docs/CONFIGURATION.md)。

准备版本、选择暂存内容并审阅后，运行 `./scripts/check --pre-commit`：
实际暂存内容的文本新增行数加删除行数最多 400 行、最多 13 个文件，含边界，并检查候选版本和适用规则。
通过且获得授权后提交；钩子再次运行统一提交前模式、Clippy 和测试，检查器也从暂存快照编译。
提交后运行普通 `./scripts/check`，仍要求工作区干净且历史版本合规；具体见[检查规则](Docs/CHECKS.md)。
Agent 开发约定以 [AGENTS.md](AGENTS.md) 为准。

## 重要工作边界

- 只有 `待办事项` 能启动新轮次；续试沿用原聊天、轮次、权限和文章，不绕过审批或输入。
- 远端发送或写入结果不明时先核对，不因重启或重复检查而盲目重放。
- 观察到匹配回合实际运行后才发布 `进行中`；收据和读回声明不能代替 Herman 的人工审核。
- 程序、配置和运行数据分开；路径依配置解析，导入配置不会复制状态或接管服务。
- 普通开发不调整运行中的服务或定时器，不让两份独立状态库同时调度同一追踪表。
- 真实服务写入、线上验证和部署需明确授权；`rpc` 仅在明确请求该 RPC 时使用。
  [线上验证工具](examples/live_validation.rs) 会写入 Notion，激活样本会触发真实执行。

## 详细文档

- [检查规则](Docs/CHECKS.md)：版本步进、Git 钩子、Rust 约束、注释和文档位置规则。
- [配置与命令](Docs/CONFIGURATION.md)：构建、首次配置、数据布局、CLI 副作用和上下文 helper。
- [业务流程](Docs/WORKFLOW.md)：调度、续试、交付收据、知识归档及真实验证边界。
- [部署与回退](Docs/DEPLOYMENT.md)：本机切换记录、线上验证证据和回退步骤。
- [迁移说明](Docs/MIGRATION.md)：原模块对应、状态兼容及移植时的验证范围。
