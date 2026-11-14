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
Cindy 已配置本机 `~/.local/bin/op-member-gh-ssh`，Git 全局 core.sshCommand 指向该入口。
GitHub 推送由既有 op-member 授权流式加载密钥到一次性 SSH agent，结束后销毁；密钥不落盘。
该入口由用户的个人 Agent 维护，本项目直接复用，不把包装器或凭据打进插件。
Fedora 的私钥与 GitHub 凭据没有复制到 Mac。
认证恢复后已实际从 GitHub 安装 Mac 0.2.98 候选，并读回服务及 MCP 版本。
本机 Agent 提交和标签使用 `~/.local/bin/agent-git`，
包装器直接调用 `/usr/bin/ssh-keygen`；提交签名与远端 SSH 认证分别验证。
普通 Git 身份保持原设置。GitHub 的 Verified 徽标另需账号登记签名公钥，
不作为这次取包和运行验证的前提。
仓库专用 HTTPS token 继续只承担读取，源码推送使用上述 SSH 入口。
2026-11-12 用户修复后，默认分支已独立读回为 main。
此前桌面 agent 拒签、API 403 及预检记录保留在迁移 handoff；不再作为当前待处理项。
Mac 真实签名提交、远端 main 写入及对应 CI 和安装结果以最终回执为准。

## 日常开发

客户端实际后台的共享入口、自动归属检查及私有 Mac 验证见
[Mac 客户端共享后台](SHARED-DESKTOP.md)。日常业务开发继续使用下面的隔离入口。

Mac 多任务隔离、串行集成与 Linux 即时候选交付的设计见
[并行开发与交付流程](PARALLEL-DEVELOPMENT.md)。第一阶段的开发入口已可用；
Codex 环境动作、串行集成和交付封装仍待实施，现行版本与完整验收规则继续有效。

2026-11-13 已在 Mac 准备两份互相独立的任务工作树：
`/Users/hongyong/Developer/codex-session-board-dev-a-20261113` 和
`/Users/hongyong/Developer/codex-session-board-dev-b-20261113`。
进入任一任务目录执行：

```zsh
./scripts/dev setup
./scripts/dev start
./scripts/dev status
./scripts/dev stop
```

首次 `start` 可自动准备环境；源码变化后先 `stop`，再 `setup`、`start`。
运行中的重复 setup/start 保持原进程；status 实时核对身份和原生依赖健康。
输出中的 URL 是 Mac 本机回环地址，不自动打开浏览器或操作日用 Codex。
每份工作树单独构建到自己的 `target/`；下载缓存复用，任务构建不自动清理共享缓存。
配置、数据库、私有 HOME 和 socket 存在 `~/.csb-dev/<工作树路径摘要>/`，
目录权限 0700，停止后保留数据。归档工作树前先停止并确认 `cleanup_confirmed=true`。

这里运行真实 Rust 看板、HTTP 与数据库，桌面/侧栏依赖连接本任务 fixture，
导航只记录到任务目录。它覆盖状态、备注、导航和重启持久性；
新建会话等未实现的 fixture 方法明确失败，不能代签真实 Codex 或远程验收。
双任务浏览器检查使用 Node.js 22+ 和 Chrome，每次创建独立 profile/CDP 端口：

```zsh
node scripts/check-dev-isolation.mjs "$URL_A" "$URL_B" "$OUTPUT"
node scripts/check-dev-isolation.mjs "$URL_A" "$URL_B" "$OUTPUT" --verify-only
```

第二条用于外部停止/重启后的只读复验，沿用首轮输出目录。
脚本操作前核对开发记录与 pid/build/dataDir；拒绝不匹配服务，报告隐藏控制 token。

本次使用 Rust 1.99.1、Python 3.12 和 Codex CLI 0.167.1。
Mac 原有 Rust 1.78.0 无法满足当前代码，已升级工具链并准备依赖缓存。
提交与验收命令在本任务目录用 zsh 执行：

```zsh
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
提交钩子通过 `--test-threads=1` 串行运行完整测试，与完整验收适配器一致。
用例内部的并发场景保持原样。SQLite writer fixture 的就绪消息单独占一行，
避免单线程 harness 的无换行前缀破坏握手；子 fixture 显式单线程覆盖该故障模式，
无需依赖 `RUST_TEST_THREADS=2`。
本机并行完整套件两次触发原生隔离耗时门禁，而同一二进制的 36 项串行复查通过；
失败日志与尚未定位的具体耗时来源继续保留，不修改断言、性能阈值或通过条件。
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
