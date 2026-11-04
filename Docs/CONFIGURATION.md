# 配置与命令

本文命令以项目根目录为工作目录。开发环境、依赖下载、Git 钩子和日常验证入口见
[README](../README.md#开发与检查)，提交及版本规则见[检查规则](CHECKS.md)。
涉及真实服务的命令应在明确授权并核对配置后执行。

## 构建与运行

构建需要 Rust/Cargo；先下载 `Cargo.lock` 锁定的依赖，再进行离线构建。
项目已在 Linux / Rust 1.99 上验证，本地套接字和进程管理针对 Linux。
Rust 主程序运行时不需要 Python 或 Python 虚拟环境，外部命令由配置指定。

开发时可以使用 debug 目标查看帮助，不连接真实服务：

```bash
cargo run --bin notion-program --locked --offline -- --help
```

部署构建使用以下命令。若本机服务引用此目录的 release 程序，重建会改变后续运行的入口，
应先按[部署说明](DEPLOYMENT.md)核对切换安排，不能作为普通文档或代码检查步骤：

```bash
cargo build --release --locked --offline
target/release/notion-program --help
```

可执行程序为 `target/release/notion-program`。以下运行示例使用这个路径；
`TASK_UUID` 是需替换的任务 UUID。

## 首次配置

独立新部署可以从示例开始。先确认目标文件不存在，不要用示例覆盖已有配置，
再复制并填写自己的数据源 ID、本地套接字和机器设置：

```bash
cp config.example.json config.json
cp archive-config.example.json archive-config.json
```

示例的 UUID 和套接字是占位值，不能直接用于连接真实服务。
归档示例默认 `enabled: false`，是否启用由部署配置决定。

也可以迁移原 Python 项目的配置：

```bash
target/release/notion-program import-config \
  --source /home/herman/Documents/Codex/2026-11-02/new-chat-2/work/notion-program \
  --destination /home/herman/文档/notion-program
```

这两个路径是本机示例，其他主机须替换。原项目只作读取对照，不修改原文件。
迁移保留模型、数据源和路由设置，将工作目录、任务目录及数据目录指向目标项目，
并将可识别的 Python WebSocket 桥替换为 Rust 原生套接字连接。
目标配置已经存在时拒绝导入。导入只迁移配置，不复制任务状态、不启动服务、不改变旧定时器。
正在使用旧状态的部署交接另见[迁移说明](MIGRATION.md)和[部署与回退](DEPLOYMENT.md)。

配置准备好后，可以只检查本地文件：

```bash
target/release/notion-program --config config.json check-config
target/release/notion-program --config archive-config.json check-config
```

本地校验不验证真实账号、远端数据源或套接字是否可用。

## 路径与数据布局

程序、配置、运行数据分别放置。`workspace`、`data_dir`、`task_workspace_root`、
`archive_data_dir`、`worker_cwd` 和 `daemon_socket` 中的相对路径，
以配置文件所在目录为基准解析；外部命令及其参数应显式配置。
默认 `--config config.json` 及 CLI 的相对 `--output`、`--source`、`--destination`
按调用时的工作目录解释，不能与上述配置字段混淆。
下表是独立新部署的示例布局，实际位置由配置决定，既有部署可能继续使用原数据目录：

| 位置 | 用途 |
| --- | --- |
| `config.json`、`archive-config.json` | 本机调度与归档配置，不作为源码提交 |
| `data/state.json` | 调度检查点，保留原 JSON 字段和阶段名称 |
| `data/events.jsonl` | 追加式事件记录 |
| `tasks/<任务 UUID>/` | 每项任务的快照、执行意图、结果和诊断 |
| `data/archive-runtime/` | 归档检查点、写入意图、读回和收据 |

`daemon_socket` 指向持久 App Server 的 Unix WebSocket 套接字。
也支持通过 `codex_command` 数组启动 JSON 行协议进程，供其他部署方式或离线测试使用。
桌面聊天兼容路径使用 `desktop_tools_command` 和 `desktop_caller_thread_id`；
这些本机信息应放在配置里。
Notion MCP 使用已经连接的账号；独立 REST 客户端由调用者传入凭据。

## 常用命令及副作用

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

- `--help` 不连接服务；`check-config` 和 `status` 只读本地文件。
- `probe` 和 `inspect` 只读远端，建立工具上下文但不启动模型回合。
  `probe` 会保存本地 `data_dir/live-fetch-probe.json`；
  `inspect --output` 会写指定的本地输出文件。
- `run` 和 `archive run` 执行实际工作，可能启动模型回合或写入 Notion。
  省略子命令时默认执行一次 `run`，兼容原入口的 `--config` 和 `--trigger` 用法；
  `manual_test` 只是触发来源标签，不是试运行开关。
- `archive run --probe` 跳过执行器和通知的健康状态退避，尝试一次正常归档周期，
  仍受 `enabled` 设置及锁等约束，可能写远端或发通知，不能当作顶层的只读 `probe`。
- `archive context` 返回项目摘要、版本和专题入口并保存本地输出；
  必要时会创建项目上下文页、在项目页补充入口链接，可能写入 Notion，不是只读命令。
  归档的 `enabled: false` 不会阻止这个上下文入口。
- `rpc --method ... --params ...` 直接发送指定的 App Server RPC，可能产生远端写入。
  这是运维入口，仅在明确请求该 RPC 时使用。
- `bridge --socket ...` 将标准输入/输出 JSON RPC 转发到本地 WebSocket，
  实际影响取决于转发的请求。

一次性演示、历史事故修复和旧部署切换脚本中的固定任务 ID 不作为默认操作；
使用 `inspect`、本地状态和明确指定的运维请求按实际对象核对。

## 项目上下文 helper

调度配置可以启用以下 helper；示例中的绝对路径须替换为实际程序和配置：

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

执行者会在这些参数后追加 `context --task ... --output ...`。
项目上下文契约和 helper 命令保存在轮次记录中，续接沿用该记录，
不能通过改动当前配置跳过原有约束；这不表示所有运行配置都会冻结。
上下文读取、知识归档及收据的关系见[业务流程](WORKFLOW.md)。
