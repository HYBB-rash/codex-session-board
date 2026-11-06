# Codex 会话看板

本仓库的当前功能是 Codex 会话状态管理。Rust 后端自动发现本机未归档交互会话，
保存人工状态和备注，观察真实执行阶段，并同步原生会话标题。
界面沿用经过原型验证的 HTML 与随包 MCP App SDK。

状态按待判断、进行中、待审阅、已阻塞、待归档、已完成排列。
开始与恢复执行进入进行中；等待批准、交互输入、失败、中断及系统异常进入已阻塞；
正常轮次结束进入待审阅。人工选择保持到新的真实执行阶段。
待归档和已完成由人工确认。看板操作不发送消息、启动任务或真正归档会话。
待归档列按卡片显示的会话更新时间正序排列，最早的在顶部；其他列保持最近更新在前。

```bash
cargo build --locked --offline
cargo run --locked --offline -- --help
cargo run --locked --offline -- launch
```

首次启动使用 XDG 配置和数据目录。配置示例见 `config.example.json`，
自定义部署使用 `--config /path/to/config.json`。
`serve` 前台运行，`launch` 复用或启动匹配服务，`stop` 核验身份并等待实际退出。
`mcp` 提供 stdio MCP App，`scan` 执行一次发现，`migrate --source-config PATH`
使用 SQLite 备份导入旧原型，保留原库并拒绝覆盖目标。

构建插件运行 `bash scripts/build-plugin`，插件清单指向生成的 Rust 二进制。
迁移说明见 [Docs/MIGRATION.md](Docs/MIGRATION.md)，
验收规范见 [acceptance/README.md](acceptance/README.md)。

```bash
cargo clippy --all-targets --locked --offline -- -D warnings
cargo test --all-targets --locked --offline
python3 scripts/verify_acceptance.py --scope automated --report /tmp/session-board-automatic.json
python3 scripts/verify_acceptance.py --evidence /path/to/evidence.json --report /tmp/session-board-release.json
./scripts/check
```

验收使用指定会话交付的 acceptance-v1，31 条需求与独立模型保持原始 SHA256。
业务测试调用 Rust 实现；Python 仅用于独立验收工具和参考模型。
自动检查通过不能替代原生 Codex 界面实测。完整验收退出 0 且
`release_ready=true` 才达到开发完成标准。

原 Notion 调度与知识归档源码保留在 Git 历史 `8f86014`，不再属于当前程序。
原运行配置与数据仍保留在工作目录的忽略路径中。
重构不安装或重启旧的 systemd 调度服务。

## 新建会话

状态导航右侧的“＋ 新建会话”按钮打开 Codex 空白会话输入页，
由用户选择项目并输入消息。按钮复用配置中的 `open_command`，
不自动发送消息或启动模型；输入页尚未发送消息时不会生成看板卡片。
原生看板与备用网页均提供此入口。
