# Mac 开发与迁移

2026-11-11 的目标机器已核对为 hongyong 的 MacBook Pro，Apple Silicon、zsh。
开发目录为 `/Users/hongyong/Developer/codex-session-board`。
迁移保全目录为 `/Users/hongyong/Developer/codex-session-board-migration/20261111`。
活动配置、凭据、用户数据库及原始聊天快照均不写进可分发插件。
附带的源码包保留仓库中的项目文档与任务上下文。
文稿目录在验证时出现额外“2”后缀副本，触发源码变更保护，来源未实锤。
现已在 Developer 新建干净 Git 检出；原文稿目录、全部副本及 Git 引用继续保全。

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
Mac 拉取源码和插件使用仓库专用 HTTPS 凭据配置，
从本机既有 `op-member` 授权读取只读 token。
配置仅保留凭据引用，token 经进程管道传给 Git。
源码推送保留已有 SSH 身份和 `ssh.github.com:443`，
已由原来受信任的 GitHub 主机密钥核对并记录该端点。
SSH agent 拒绝签名属于认证失败，须在 Mac 的凭据管理器中允许 Git 访问。
Fedora 的私钥与 GitHub 凭据没有复制到 Mac。
认证恢复后已实际从 GitHub 安装 Mac 0.2.98 候选，并读回服务及 MCP 版本。
本机 Agent 提交和标签使用 `~/.local/bin/agent-git`，
包装器直接调用 `/usr/bin/ssh-keygen`；提交签名与远端 SSH 认证分别验证。
普通 Git 身份保持原设置。GitHub 的 Verified 徽标另需账号登记签名公钥，
不作为这次取包和运行验证的前提。
当前保存的 token 可读取本私有仓库，推送和修改默认分支返回 403。
2026-11-12 已从 Mac 完成 HTTPS 拉取及 SSH 推送预检。
预检曾记录一把 agent 密钥拒签，随后同次调用成功；不据此推断所有密钥均已解锁。
真实 Agent 签名源提交与推送结果以最终回执为准。

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

`./scripts/check --prepare-commit` 同步 Cargo 与两个插件清单的版本。
提交前须审阅并暂存本次代码和四份版本文件，再运行
`./scripts/check --pre-commit`；源提交使用本机独立 Agent 签名器。
Mac 的密钥保持本机管理。源码规则不因迁移放宽，完整验收仍使用原 31 条需求、模型和锁。
测试临时目录使用 `/private/tmp`，避免 `/tmp` 符号链接和过长 Unix socket 路径。
macOS 与 Linux 的 socket 队列差异采用对应的明确结果断言，原测试编号保留。
CI 的 19 项钩子集成测试逐项使用独立编译目录；Mac runner 曾在 600 秒内完成前 17 项。
该测试进程的执行预算为 1200 秒，其余测试进程保持 600 秒，业务断言保持原值。
试用编译缓存出现零命中，已移除该依赖。

## 获取与验证新版本

发布者从同一个已提交源版本，在 Linux 和 Mac 各构建本机包。
每个包带平台、源提交、源码指纹、可执行文件与资产摘要。构建和发布命令见
`Docs/DEPLOYMENT.md`；消费者通过同一插件 ID 选择本机平台分支。
开发者可使用 candidate，完成目标环境签收后才提升 stable。
每个原生插件包含 `SOURCE.tar.gz` 及其 SHA256 文件；源码包沿用既有迁移白名单。
解包到独立目录后，可运行 `python3 scripts/verify_acceptance.py` 复验同一源码指纹。
没有外部证据时，完整验收仍返回待验证；准备 Rust 依赖后可单独运行 automated 范围。

GitHub 对本次源码的双平台构建通过后，在 Mac 开发目录执行：

```zsh
git pull --ff-only
./scripts/update-plugin --channel candidate
```

命令从 origin 获取正确平台的包，候选源码必须与本地 HEAD 相同；
CI 尚未产出对应版本时，命令在停止现有服务前报错。
固定旧 revision 回退可传 `--revision SHA`，此时按显式选择的旧源版本验证。
存在外部 `~/.config/codex-session-board/gitconfig` 时，
更新命令仅在子进程中加载它。开发检出本地 include 同一份配置；
普通用户全局 Git 配置未改。

安装状态放在 `~/.local/share/codex-session-board/installation`，
配置放在 `~/.config/codex-session-board/config.json`，数据放在独立持久目录。
首次 Mac 验证使用新配置，保持标题同步、分区写入与自动归档关闭。
生产配置和旧数据库由原部署继续持有；本任务没有将其覆盖成 Mac 测试数据。
现有原生分区和远程连接需要按实际客户端读回配置并完成验收。

检查 `current.json` 中的 `phase`、`revision`、`release`、`runtime` 和 `mcp`；
`phase=complete` 表示安装、服务身份及 MCP 版本核对完成，
仍不代表完整业务外部验收。若存在 pending，先查服务日志与原失败，不重复宣称成功。
旧版本程序、旧数据目录和备份在人工确认前保留。
更新通过原生 `config/mcpServer/reload` 刷新配置，再定向读取 `session_board` 的实际版本和工具。
全局 `skills/list forceReload` 在 Mac 曾连续超时，已移除对该无关扫描的依赖；
定向重载和读回实测约 1.5 秒。原失败、pending 状态和后续恢复记录一并保留。
原生市场添加时直接固定预取并校验过的平台包 Git revision，随后安装该快照。
不再紧接着再次刷新新添加的市场；该重复下载曾两次以 `early EOF` 失败。
后续更新由本命令重新解析 candidate 分支，并为每次安装选择相应的已校验快照。
原生安装来源通过市场实际 Git HEAD 与缓存全部文件哈希核对。
本次 Codex 0.167.1 的正常安装缓存没有内部安装标记；存在标记时还会核对其一致性。
真实新会话、原会话跳转、侧栏和远程状态等证据必须绑定同一个候选包及二进制 SHA。

本次执行的最终回执保存在任务输出目录及 Mac 迁移保全目录。
最新源提交、平台分支与回退 revision 以回执为准。
