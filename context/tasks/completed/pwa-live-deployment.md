# PWA 日常看板部署

owner: codex
execution_mode: isolated_write
allowed_write_set: Cargo.toml, Cargo.lock, context/, bin/, outputs/, 现有看板受管服务、现有 Rust 看板插件部署副本
base_commit: 48ee1e16bae9399cab9a37967f9887a509d2a973
worktree_path: /home/herman/.codex/worktrees/pwa-support/notion-program
coordinator: codex-pwa

2026-11-07 用户授权将 main 当前 PWA 版本部署到正在使用的看板，供其安装体验。
沿用原生产配置、端口 18767 和数据目录，备份二进制、插件、配置与 SQLite。
通过现有启动器核验并停止旧服务，再启动新程序，保持已有状态和备注。
同步部署清单与二进制版本；不启用旧 Python 看板或 Notion 服务。
在实际 HTTP 服务验证 PWA 清单、Service Worker、安装资格和安装按钮。
不代替用户执行系统安装，不把本次部署等同于完整 31 条发布验收。

已按仓库补丁规则准备 0.1.30，保留 main 的功能代码与冻结契约。
旧程序经启动器核验停止，新服务仍使用原配置和端口。
marketplace 与安装缓存均更新至 0.1.30，保留显式生产配置注入，
执行 skills/list forceReload 与 config/mcpServer/reload。
服务、安装缓存二进制 SHA256 与服务 build 字段一致。
118 条会话和全部旧备注保留，扫描、运行观察与状态分区同步正常。
用户 Chrome 已打开实际网页，安装应用按钮可见，清单与安装资格无错误，
Service Worker 已控制页面，六列显示正常；页面保留给用户自行试装。
备份与原始证据位于 outputs/pwa-live-20261107/，截图位于源码外证据目录。
生产配置不变，116 条人工状态保留；MCP 服务器确认加载 0.1.30。
254 项自动检查通过，完整验收退出 2、release_ready=false，未代签其他证据。
