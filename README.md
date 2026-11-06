# Codex 会话看板

本仓库的当前功能是 Codex 会话状态管理。Rust 后端自动发现本机未归档交互会话，
保存人工状态和备注，观察真实执行阶段，并同步原生会话标题。
显式开启分区同步后，原生侧边栏的六个分区也表示同一套状态，会话随状态移动。
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

## 项目筛选与搜索

顶部默认显示项目下拉框，包含全部项目、默认工作区与本机 Codex 当前加载的项目。
旁边的按钮切换到标题、路径搜索；搜索覆盖全部会话，两种模式分别保留当前输入。
没有关联项目的会话归默认工作区。移除项目后会话自动归默认，重新添加相同目录后
自动识别，状态和备注保持原样。同名项目显示目录以便区分，空项目也保留入口。
多根目录、嵌套项目、符号链接与外部 Git 工作树的规则见
[Docs/PROJECT-FILTERS.md](Docs/PROJECT-FILTERS.md)。

## 新建会话

状态导航右侧的“＋ 新建会话”按钮打开 Codex 空白会话输入页，
由用户选择项目并输入消息。按钮复用配置中的 `open_command`，
不自动发送消息或启动模型；输入页尚未发送消息时不会生成看板卡片。
原生看板与备用网页均提供此入口。

## 状态分区

`sync_sections` 默认关闭。启用时配置 `sidebar_socket`、调用方已有会话
`sidebar_caller`、客户端全局状态文件 `sidebar_state_file` 和六状态到分区 UUID
的 `status_sections`。全局状态文件仅用于读取已加载会话的内部别名，不改写它。
部署机器的 socket、调用方和分区 ID 均保存在外部配置，不写入源码。
首次迁移先备份原生布局，创建六个同名状态分区，并核实会话位置后撤掉旧分区。
项目本身保留在项目列表；固定等内置入口由客户端维护。

保存人工状态时立即尝试移动，后台按 `runtime_poll_seconds` 对账原生位置。
标题同步和分区同步独立开关。已有状态在失败和重启后仍用于补偿，
只移动本轮完整发现的未归档会话，不根据缓存记录恢复已归档会话。
分区缺失、改名或连接异常会显示待重试，不创建或删除分区。
本轮正在执行的请求与等待间隔共同决定补偿耗时，轮询间隔不是时限保证。
验收边界及本次用户授权见 [Docs/STATUS-SECTIONS.md](Docs/STATUS-SECTIONS.md)。
