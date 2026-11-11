# 构建与部署

原生构建目标为 Linux x86_64 GNU 与 macOS Apple Silicon。
Linux 使用 `/proc` 与 pidfd；macOS 使用同用户控制 socket、自身停止和 kqueue 退出确认。
本次 CLI 接入使用 Linux Codex 0.167.0、Mac Codex 0.167.1。构建候选与完整目标环境签收分别记录。
Mac 的开发入口与迁移记录见 `Docs/MAC-DEVELOPMENT.md`。

```bash
cargo fetch --locked
plugin_dir="$(bash scripts/build-plugin)"
python3 scripts/publish_plugin.py --repository "$PLUGIN_REPOSITORY" \
  --package "$plugin_dir" --channel candidate
```

`PLUGIN_REPOSITORY` 是私有 Git 产物仓库。源码和产物可用同一 Git 远端的不同分支，
或使用两个独立远端。市场名固定为 `session-board-rust`，插件 ID 固定为
`codex-session-board@session-board-rust`。每个平台使用 `candidate/<Rust target>`、
`stable/<Rust target>` 分支，同一机器只能选择与本机相符的平台。
发布脚本只接受干净检出中的原生构建，逐项核对版本、源码提交、源码指纹、
冻结需求摘要和包内文件 SHA256；`RELEASE.json` 不装入实际配置或数据库。
已有分支按正常 Git 快进提交更新，远端读回确认 SHA 后才报告发布成功。
重复发布相同包保持同一提交。

`.github/workflows/plugin-candidate.yml` 在两个原生 runner 上运行检查、自动验收、
构建和候选发布，保存可下载产物。它没有自动提升稳定版本的步骤。
云端工作流必须在实际私有仓库上运行通过后才能记为已验证。
每个平台版本绑定固定产物，重复发布同一包保持不变；
相同版本的不同产物会被拒绝。默认由 CI 发布，手动发布使用新的版本。

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
更新工具使用当前守护进程的私有 Unix WebSocket 完成插件重载与版本读回。
桌面 socket 可以使用进程环境中的 `CODEX_LINUX_APP_SERVER_BRIDGE_SOCKET`，
也可以写入部署配置。更换用户或机器时只调整外部配置。
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

`serve` 在当前进程监听，只复用相同数据目录、配置和构建的服务。
插件的 `mcp` 使用同一规则，不另起进程。
变更配置、端口或二进制后，先 `stop` 再启动。
活进程暂时不可访问时拒绝再开一份。`stop` 核对 PID、启动时间和可执行文件身份。

## 更新与回退

安装更新工具依赖：

```bash
python3 -m venv ~/.local/share/codex-session-board/delivery-venv
~/.local/share/codex-session-board/delivery-venv/bin/python -m pip install -r requirements-delivery.txt
source ~/.local/share/codex-session-board/delivery-venv/bin/activate
```

先准备独立配置，确认 `data_dir` 指向需保留的运行数据，再执行：

```bash
python3 scripts/update_plugin.py --repository "$PLUGIN_REPOSITORY" \
  --channel candidate --config "$HOME/.config/codex-session-board/config.json"
```

脚本取得匹配本机架构的包，验证可执行版本，核实并停止原进程，
使用 SQLite backup API 保全主库及各远程主机库的已提交 WAL 数据，
再更新 Git 市场与插件缓存。安装器保留配置、旧包和备份，不自动清理历史 release。
历史 data_dir 即使位于旧 release 下也不得随程序清理删除。
启动后读回真实 PID、版本、二进制 SHA、数据目录，再强制刷新技能及 MCP；
MCP 清单中的插件 ID、serverInfo.version 和 board_view 必须吻合。
`current.json` 保存回执；失败保留 `pending.json` 与 `service.log`，不能解释成更新成功。
`--defer-reload` 只记录 `awaiting_client_reload`。
Git 开发检出也可运行 `./scripts/update-plugin --channel candidate`：
自动读取 origin、外部配置和本地 HEAD，CI 尚未产出该 HEAD 时不停止原版本。

将上次回执中的 Git `revision` 固定后，回退命令为：

```bash
python3 scripts/update_plugin.py --repository "$PLUGIN_REPOSITORY" \
  --revision "$PREVIOUS_REVISION" --config "$HOME/.config/codex-session-board/config.json"
```

回退保留当前数据库。数据库格式若有不兼容升级，应先按该升级的迁移文档恢复备份；
固定程序版本不等于任意格式都可逆。
局域网 SSH 需专用客户端配置时，可向发布或更新脚本传 `--ssh-config PATH`。

## 稳定发布

完整 31 条需求与外部环境证据按现有 `scripts/verify_acceptance.py`、
`scripts/verify_remote_release.py` 验证。审阅对应证据文件的实际内容后，将同一个包提升：

```bash
python3 scripts/publish_plugin.py --repository "$PLUGIN_REPOSITORY" \
  --package "$plugin_dir" --channel stable --evidence "$REVIEWED_EVIDENCE"
```

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
