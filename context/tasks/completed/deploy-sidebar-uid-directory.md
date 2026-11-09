# 部署连接目录兼容修复

owner: codex
execution_mode: serial_write
allowed_write_set: context/, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, outputs/, bin/, 正式看板插件与marketplace配置、受管服务
base_commit: 26fab5c98b37489c3d4a7cd963dc61b760c3db0d
worktree_path: /home/herman/文档/notion-program
coordinator: codex

2026-11-09 用户明确授权将已提交的兼容修复替换上线，不继续运行旧实现。
构建并核验 v0.2.40，备份正式配置和运行数据，保留原数据目录与行为配置，
更新正式服务、插件包、缓存及实际 Native/MCP 入口，核对运行字节和状态同步。
保留原完整验收边界，不将旧证据签给新构建，不将上线等同完整发布验收。

实际已部署 v0.2.40，源码 26fab5c，binary SHA256：
025818455385153fd8077e84e6097040f9ac44d4bda685551778dfc950df75a9。
正式 package、cache、HTTP 18767 进程与 4 个 MCP 实例字节一致；
插件 installed/enabled、MCP serverInfo.version 及 Native 资源均读回 40。
新配置为 /home/herman/.config/codex-session-board/config-0.2.40.json，
字段与部署前完全相同，数据保留原目录，没有执行库迁移。
5 个库完整性通过，原有 146/100/0/0/44 条备注字段逐项一致。
本会话处于进行中，nativeStageError 清除，4 个历史位置缺项保留。

实际 release binary 的隔离 MCP 实测在旧目录不存在时从 UID 目录
完成只读 list_projects；版本、binary hash、隔离数据路径均符合，
未产生任何会话，验证服务退出并清理。原 487 项自动检查通过。
插件安装事务曾重命名预置 cache inode；逐一核对 pidfd、启动时间、
运行字节与 HTTP 身份后清理这些 deleted 路径实例，再从最终缓存启动。
原 38 正式进程已退出，保留的旧启动入口仅链接新 binary，不再启动旧字节。
全部证据与私有备份在 outputs/sidebar-uid-deployment-20261109/。
全量 31 条外部签收仍待补，不冒称 release_ready=true。
本次 v0.2.41 只提交部署上下文；实际运行和资源固定为上述 40 产物。
