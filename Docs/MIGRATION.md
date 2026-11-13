# Rust 重构与验收

2026 年 11 月 6 日，用户确认把当前仓库整体改成会话看板。
新程序名为 `codex-session-board`，版本仍沿用本仓库当前版本。
2026 年 11 月 7 日，用户改为由 Agent 自主决定本地提交，不再逐次申请授权。
准备提交时按仓库规则推进版本，推送和部署按对应任务的授权范围执行。

验收来源是 Codex 会话 `01a1ac96-0282-7871-ae59-1d2ce1a8583e`
交付的 `codex-session-board-0.2.5-acceptance-v1.tar.gz`。
原包及 SHA256 位于 `Docs/verification-baseline/`。
迁移时 `acceptance/requirements.json`、`model.py` 与 `contract.lock.json` 原样保留，
Rust 测试与绑定替换原型测试的接线。2026-11-13 用户批准 UI-002 改为阻塞原因选填，
契约升级为 v2 并同步需求锁；31 个编号、其他 30 条及独立参考模型不变。
原 v1 包继续保留在上述基线目录。此次调整的行为及证据要求见 `acceptance/README.md`。

运行代码全部使用 Rust。HTML、CSS、浏览器 JavaScript 与离线 MCP App SDK 复用。
验收执行器继续使用 Python 标准库，与运行时分离。
配置默认使用 `$XDG_CONFIG_HOME/codex-session-board/config.json`，
数据库默认使用 `$XDG_DATA_HOME/codex-session-board/status.sqlite3`。
未设置 XDG 时分别回退至用户目录下 `.config` 和 `.local/share`。
`CODEX_SESSION_BOARD_CONFIG` 可覆盖配置路径，配置内的相对数据路径相对配置目录解析。
桌面连接来自配置或 `CODEX_LINUX_APP_SERVER_BRIDGE_SOCKET`。

原型数据导入使用：

```bash
cargo run --locked --offline -- --config /path/to/new/config.json migrate --source-config /path/to/old/config.json
```

原配置和原库保留。目标数据库存在时拒绝覆盖。程序更新不把配置或数据写入插件缓存。
部署前应核对当前服务，服务身份或构建不匹配时显式停止旧服务后再启动。

远程接入后，迁移同时枚举根库与磁盘上的全部 remote-hosts 子库，
包括已经禁用或移除登记的主机。SQLite Backup 包含已提交的 WAL，
只额外保留来源身份与分页进度，不复制锁、socket、进程记录或凭据。
源库、侧车和目标冲突先检查，配置最后发布，正常失败只回滚本次创建项。
每库快照各自一致，不构成跨库同一时刻的事务；需要统一时点时先协调暂停写入方。
进程被杀或掉电后的遗留不能靠覆盖重试，应先审阅备份与目标。

旧本机 auto_archive_seconds 不自动启用远程归档，
remote_auto_archive_seconds 默认 null。冻结六状态需显式 legacy_mode=true，
与日常原生五状态分别验收，不能用兼容模式或旧安装包签收新路径。
完整 release 与原生/远程汇总的源码指纹和 executable SHA256 必须相同。
build-plugin 使用实际 CARGO_TARGET_DIR 的新构建，避免打包另一路径的旧二进制。
构建门禁 build.rs 是显式必需输入，纳入迁移包及验收指纹；删除脚本不能绕过门禁。
浏览器验证所需的 scripts/*.mjs 同样随源码包提供。
完整串行验收的编译、全部 Rust binary 和 Python 测试共用 1800 秒外层预算；
单条内部命令仍限 600 秒，测试项、断言及验证器的 3600 秒上限不变。
先前串行提交检查的各测试程序累计约 912 秒，不能用单条命令预算截断完整套件。

当前仓库仍使用原 `scripts/check`、注释限制、Clippy 和 Git 钩子。
开发改动尚未提交时，工作区干净检查会失败，不可用忽略文件或重置工作区掩盖。
Agent 自主按每次最多 13 文件、400 行和每次提交自动进位规则分步提交。
2026 年 11 月 7 日起第三段满 100、第二段满 10 自动向前进位，
不再等待人工宣布阶段完成。旧历史按固定切换点兼容，详见 `Docs/CHECKS.md`。
