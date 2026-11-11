# Mac 开发与迁移

2026-11-11 的目标机器已核对为 hongyong 的 MacBook Pro，Apple Silicon、zsh。
开发目录为 `/Users/hongyong/Documents/codex-session-board`。
迁移保全目录为 `/Users/hongyong/Documents/codex-session-board-migration/20261111`。
机器地址、配置与凭据均不写进可分发插件。

## 开发上下文

先读 `AGENTS.md`、`context/README.md`、`context/PROJECT.yaml`、
`context/STATE.yaml`，再读本任务卡、handoff 和相关资料。
原检出仍有独立任务，迁移实现采用基于已部署 f9c68da 的独立 clone；
合并由原工作区协调者处理，不能用此 clone 覆盖原工作区。

首次保全包括全部 Git 引用、stash、5 个当时工作区 HEAD 与脏改动，
133 份相关开发会话，以及项目文档和开发证据。清理任务另存的实验工作区
与历史 Git bundle 也已迁入。快照不包含 Rust target 缓存、机器凭据或生产配置。
每份归档和工作区文件清单均带 SHA256；恢复脏工作区前先核对对应 HEAD 和 patch，
在独立检出中恢复，不把 patch 直接叠加到当前 main。
`history.bundle` 保留的是 Git 历史；会话原文件是开发记录，不自动合并到 Mac 的实时会话索引。

GitHub 最初建库返回 HTTP 403，用户修复权限后已创建私有仓库
`HYBB-rash/codex-session-board`。源码使用 main，产物使用平台分支。
临时局域网 `source.git`、`plugins.git` 与迁移 bundle 继续保留。
准确源 SHA、平台 revision 和最终推送结果以任务回执为准。
Mac 访问 GitHub 使用已有 SSH 身份和 `ssh.github.com:443`；
已由原来受信任的 GitHub 主机密钥核对并记录该端点。
SSH agent 拒绝签名属于认证失败，须在 Mac 的凭据管理器中允许 Git 访问。
Fedora 的私钥与 GitHub 凭据没有复制到 Mac。

## 日常开发

本次使用 Rust 1.99.1、Python 3.12 和 Codex CLI 0.167.1。
Mac 原有 Rust 1.78.0 无法满足当前代码，已升级工具链并准备依赖缓存。
命令在开发目录用 zsh 执行：

```zsh
git pull --ff-only
./scripts/install-git-hooks
cargo fetch --locked
cargo clippy --locked --offline --all-targets -- -D warnings
python3 scripts/verify_acceptance.py --scope automated --report outputs/mac-automated.json
./scripts/check
```

更新工具的 Python 环境独立于源码和运行数据：

```zsh
python3 -m venv ~/.local/share/codex-session-board/delivery-venv
~/.local/share/codex-session-board/delivery-venv/bin/python -m pip install -r requirements-delivery.txt
```

提交前仍须准备版本、同步两个插件清单、审阅暂存内容并运行
`./scripts/check --pre-commit`；源提交使用本机独立 Agent 签名器。
Mac 的密钥保持本机管理。源码规则不因迁移放宽，完整验收仍使用原 31 条需求、模型和锁。
测试临时目录使用 `/private/tmp`，避免 `/tmp` 符号链接和过长 Unix socket 路径。
macOS 与 Linux 的 socket 队列差异采用对应的明确结果断言，原测试编号保留。

## 获取与验证新版本

发布者从同一个已提交源版本，在 Linux 和 Mac 各构建本机包。
每个包带平台、源提交、源码指纹、可执行文件与资产摘要。构建和发布命令见
`Docs/DEPLOYMENT.md`；消费者通过同一插件 ID 选择本机平台分支。
开发者可使用 candidate，完成目标环境签收后才提升 stable。

GitHub 对本次源码的双平台构建通过后，在 Mac 开发目录执行：

```zsh
git pull --ff-only
./scripts/update-plugin --channel candidate
```

命令从 origin 获取正确平台的包，候选源码必须与本地 HEAD 相同；
CI 尚未产出对应版本时，命令在停止现有服务前报错。
固定旧 revision 回退可传 `--revision SHA`，此时按显式选择的旧源版本验证。

安装状态放在 `~/.local/share/codex-session-board/installation`，
配置放在 `~/.config/codex-session-board/config.json`，数据放在独立持久目录。
首次 Mac 验证使用新配置，保持标题同步、分区写入与自动归档关闭。
生产配置和旧数据库由原部署继续持有；本任务没有将其覆盖成 Mac 测试数据。
现有原生分区和远程连接需要按实际客户端读回配置并完成验收。

检查 `current.json` 中的 `phase`、`revision`、`release`、`runtime` 和 `mcp`；
`phase=complete` 表示安装、服务身份及 MCP 版本核对完成，
仍不代表完整业务外部验收。若存在 pending，先查服务日志与原失败，不重复宣称成功。
旧版本程序、旧数据目录和备份在人工确认前保留。
真实新会话、原会话跳转、侧栏和远程状态等证据必须绑定同一个候选包及二进制 SHA。

本次执行的最终回执保存在任务输出目录及 Mac 迁移保全目录。
最新源提交、平台分支与回退 revision 以回执为准。
