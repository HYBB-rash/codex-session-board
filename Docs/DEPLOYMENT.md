# 构建与部署

运行和服务管理目前支持 Linux。进程身份检查使用 `/proc` 与 pidfd，
桌面运行观察使用当前用户私有 Unix WebSocket。
其他平台未完成运行或验收，不从配置默认值推断为已支持。

```bash
plugin_dir="$(bash scripts/build-plugin)"
codex plugin marketplace add "$plugin_dir"
codex plugin add codex-session-board@session-board-rust --json
```

安装后让 Codex 重新加载插件。最左侧应用栏由全局 `board_view` 入口提供，
前端资源随 MCP 服务返回，不嵌套备用网页。资源和备用网页通过同一个受保护服务读写。
构建脚本在 `outputs` 下生成独立部署目录，只放二进制、清单、前端资产与技能，
避免插件安装器把仓库中的实际配置、数据库和构建缓存复制进插件缓存。
MCP 命令和工作目录使用插件根目录下的相对路径。
修改插件启动配置后，当前客户端可能仍保留旧插件缓存。
本次 Linux 客户端通过 `skills/list` 的 `forceReload=true` 清除插件缓存，
随后调用 `config/mcpServer/reload`，再独立核对插件 ID、版本及工具加载错误。
正在执行的旧会话可能仍绑定原 MCP 实例，验收使用重新加载后创建的专用会话及全局入口。

配置默认在 XDG 配置目录，数据默认在 XDG 数据目录。首次 `serve` 初始化配置；
可以先复制 `config.example.json` 至独立位置，再用 `--config PATH`。
`open_command` 是客户端命令参数数组，最后追加 `codex://threads/…` 链接。
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

源码迁移包：

```bash
python3 scripts/package_migration.py --output outputs/codex-session-board-rust.tar.gz
```

归档不包含真实配置、数据库、日志、截图或构建缓存。
解包后需要 Rust 工具链和 `Cargo.lock` 对应的依赖缓存，运行
`bash scripts/build-plugin` 生成当前平台二进制。
离线前端资产随源码提供；异目录验收不表示已验证全新机器的工具链安装。

本次源码重构不更换已安装的插件，也不启用任何 systemd 服务。
原 Notion 调度服务的停用状态由原部署管理，禁止因重构而恢复调度。
