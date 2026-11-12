# 构建与部署

原生构建目标为 Linux x86_64 GNU 与 macOS Apple Silicon。
Linux 使用 `/proc` 与 pidfd；macOS 使用同用户控制 socket、自身停止和 kqueue 退出确认。
本次 CLI 接入使用 Linux Codex 0.167.0、Mac Codex 0.167.1。构建候选与完整目标环境签收分别记录。
Mac 的开发入口与迁移记录见 `Docs/MAC-DEVELOPMENT.md`。

```bash
./scripts/release candidate
./scripts/release candidate --local
```

发布入口从当前 GitHub 仓库读取身份，也可传 `--repository owner/repo`。
当前源码与渠道使用同一公开仓库；`PLUGIN_REPOSITORY` 用于安装工具的 Git 远端地址。
市场名固定为 `session-board-rust`，插件 ID 固定为
`codex-session-board@session-board-rust`。每个平台使用 `candidate/<Rust target>`、
`stable/<Rust target>` 分支，同一机器只能选择与本机相符的平台。
发布脚本只接受干净检出中的原生构建，逐项核对版本、源码提交、源码指纹、
冻结需求摘要和包内文件 SHA256；`RELEASE.json` 不装入实际配置或数据库。
已有分支按正常 Git 快进提交更新，远端读回确认 SHA 后才报告发布成功。
重复发布相同包保持同一提交。

提交并推送源码后执行统一入口。默认命令触发 `.github/workflows/plugin-candidate.yml`；
`--local` 在本机补充对应平台，已有包会直接下载复用。两个入口使用相同保存规则。
版本标签必须绑定当前提交。同一版本的源码不匹配时停止，必须准备新版本。

流程为：构建并验证 → 保存 GitHub Release 附件 → 双平台验证 → 发布候选渠道。
附件名为 `plugin-<Rust target>.tar.gz` 及其 `.sha256`，归档中保留 `RELEASE.json`。
包与 GitHub 附件摘要、源码提交、版本、平台及文件清单逐项匹配后才能复用。
并发上传同名附件时重新读取并验证已保存的包，不删除、覆盖或使用 `--clobber`。
只完成一个平台时保留草稿并报告 `incomplete`，不会更新渠道。
双平台齐全后要求仓库启用 GitHub immutable releases，再发布不可变预发布版本。
Actions Artifacts 仅保存本次验收报告；长期交付包由 Release 保存。
发布中断后重复同一入口，继续消费原包。渠道写入从包绑定的源码提交运行验证与发布器。
工作流不会自动提升稳定版本；GitHub 运行成功和完整目标环境验收分别记录。

安装后让 Codex 重新加载插件。最左侧应用栏由全局 `board_view` 入口提供，
前端资源随 MCP 服务返回，不嵌套备用网页。资源和备用网页通过同一个受保护服务读写。
构建脚本在 `outputs` 下生成独立部署目录，只放二进制、清单、前端资产与技能，
避免插件安装器把仓库中的实际配置、数据库和构建缓存复制进插件缓存。
MCP 通过包内 `bin/run-plugin` 进入独立数据根目录后启动。
客户端跳转子进程使用解析后的数据目录，退出错误保留状态码和工作目录。
删除源 worktree 或旧构建目录不会使运行进程依赖的 cwd 消失。
修改插件启动配置后，当前客户端可能仍保留旧插件缓存。
本次 Linux 客户端通过 `skills/list` 的 `forceReload=true` 清除插件缓存，
随后调用 `config/mcpServer/reload`，再独立核对插件 ID、版本及工具加载错误。
正在执行的旧会话可能仍绑定原 MCP 实例，验收使用重新加载后创建的专用会话及全局入口。

配置默认在 XDG 配置目录，数据默认在 XDG 数据目录。
可以先复制 `config.example.json` 至独立位置，再用 `--config PATH`。
`open_command` 是客户端命令参数数组，最后追加 `codex://threads/…` 链接；
省略该字段时 macOS 使用 `open`，Linux 使用 `xdg-open`。
`codex_command` 默认使用 `codex app-server --stdio` 读取历史；
GUI 的 PATH 不含 CLI 时，在机器配置中填写实际 CLI 绝对路径。
更新工具通过桌面客户端提供的私有 Unix WebSocket 完成插件重载与版本读回。
更新工具只从进程环境中的 `CODEX_LINUX_APP_SERVER_BRIDGE_SOCKET` 或 Linux 标准运行目录
取得桌面 socket；未发现桌面连接时保留待加载状态。
若 MCP 子进程没有继承桌面 socket 环境变量，应在同一外部配置中明确填写
`desktop_socket`，使 MCP 与命令行启动时解析出相同的配置身份。

如果已有 Python 原型在运行，先使用原型启动器停止，并保留原配置和数据库备份。
不要用新启动器强行接管缺少进程身份字段的旧记录。
导入至新的目标配置使用 `migrate --source-config PATH`。
已有配置或目标数据库会触发拒绝覆盖，具体步骤见 `Docs/MIGRATION.md`。

```bash
bin/codex-session-board serve
bin/codex-session-board stop
```

`serve` 用于明确的独立部署，在当前进程监听。普通桌面安装由 `mcp` 自己持有业务运行时和 HTTP 线程。
多个插件入口通过同一配置入口锁和数据目录锁共享一个所有者，不创建分离的后台服务。
变更配置、端口或二进制后，先 `stop` 再启动。
活进程暂时不可访问时拒绝再开一份。`stop` 核对 PID、启动时间和可执行文件身份。

配置迁移、运行时退出和安装回执的结构见 `Docs/PLUGIN-RUNTIME.md`。

## 更新与回退

安装更新工具依赖：

```bash
python3 -m venv ~/.local/share/codex-session-board/delivery-venv
~/.local/share/codex-session-board/delivery-venv/bin/python -m pip install -r requirements-delivery.txt
source ~/.local/share/codex-session-board/delivery-venv/bin/activate
```

普通安装和更新沿用插件统一解析的正式配置入口。更新命令不再选择配置：

```bash
python3 scripts/update_plugin.py --repository "$PLUGIN_REPOSITORY" \
  --channel candidate
```

脚本取得匹配本机架构的包，验证可执行版本，更新 Git 市场与插件缓存，
再请求实际桌面客户端重载。新运行时打开数据库前通过 SQLite backup API
保全主库及各远程主机库的已提交 WAL 数据。安装器保留配置、旧包和备份，不自动清理历史 release。
历史 data_dir 即使位于旧 release 下也不得随程序清理删除。
重载后核对实际桌面的 MCP 清单；
MCP 清单中的插件 ID、serverInfo.version 和 board_view 必须吻合。
`current.json` 保存安装和加载回执；失败保留 `pending.json`，不能解释成桌面已加载。
`--defer-reload` 只记录 `awaiting_desktop_reload`。
Git 开发检出也可运行 `./scripts/update-plugin --channel candidate`：
自动读取 origin 和本地 HEAD，CI 尚未产出该 HEAD 时不修改已安装版本。
可用外部 `~/.config/codex-session-board/gitconfig` 配置仓库专用认证，
启动器将其作为此次更新子进程的 `GIT_CONFIG_GLOBAL`；
手动调用 Python 发布或更新工具时，也可为单条命令设置该变量。

将上次回执中的 Git `revision` 固定后，回退命令为：

```bash
python3 scripts/update_plugin.py --repository "$PLUGIN_REPOSITORY" \
  --revision "$PREVIOUS_REVISION"
```

回退保留当前数据库。数据库格式若有不兼容升级，应先按该升级的迁移文档恢复备份；
固定程序版本不等于任意格式都可逆。
局域网 SSH 需专用客户端配置时，可向发布或更新脚本传 `--ssh-config PATH`。

## 稳定发布

完整 31 条需求与外部环境证据按现有 `scripts/verify_acceptance.py`、
`scripts/verify_remote_release.py` 验证。审阅对应证据文件的实际内容后，将同一个包提升：

```bash
./scripts/release stable --evidence "$REVIEWED_EVIDENCE_INDEX"
```

证据索引为 JSON，`platforms` 按 Rust target 指向对应平台已审阅的证据文件：

```json
{"platforms":{"x86_64-unknown-linux-gnu":"linux.json","aarch64-apple-darwin":"mac.json"}}
```

路径相对于索引文件。每份证据仍须满足现有验证器对源码、二进制和目标环境的约束。
两平台都验证通过后才写入稳定分支；不重新编译或替换 Release 附件。
成功后解除预发布标记。若某个平台渠道写入失败，重复命令继续，已完成的平台保持原包。

完整验证必须退出 0 且 `release_ready=true`；缺证时稳定发布被拒绝。
自动验收通过、CLI 接收跳转或服务启动，均不能替代真实客户端和远程验收。

源码迁移包：

```bash
python3 scripts/package_migration.py --output outputs/codex-session-board-rust.tar.gz
```

归档不包含真实配置、数据库、日志、截图或构建缓存。
解包后需要 Rust 工具链和 `Cargo.lock` 对应的依赖缓存，运行
`cargo build --release --locked --offline` 生成当前平台二进制。
可发布插件包从干净 Git 检出运行 `bash scripts/build-plugin`；
完整开发迁移另保留 `history.bundle`，可恢复同源 Git 历史再构建发布包。
离线前端资产随源码提供；异目录验收不表示已验证全新机器的工具链安装。

本次源码重构不更换已安装的插件，也不启用任何 systemd 服务。
原 Notion 调度服务的停用状态由原部署管理，禁止因重构而恢复调度。
