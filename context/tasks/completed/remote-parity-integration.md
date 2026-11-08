# 正式接入远程会话并验收，2026-11-09 已部署

owner: codex
execution_mode: coordinated_isolated_workers
allowed_write_set: src/, tests/, assets/, examples/, scripts/, Docs/, acceptance/bindings.json, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, config.example.json, README.md, context/, outputs/remote-parity-integration/
base_commit: 4187733
worktree_path: /home/herman/.codex/worktrees/remote-parity-lab/notion-program
coordinator: codex

用户授权正式接入、功能对齐及完成验收。复用实验及现有业务逻辑。
已将实验移到主线 218e78b，保留崩溃恢复、定时归档、排序及项目任务取消记录。
原实验提交和证据仍可通过 49e9057 及 outputs/remote-parity-lab/ 查询。

依次实现受限共享代理传输、按主机业务实例与数据接入、发现及运行观察、
原生映射与交互、真实验收和部署核验。不得降低冻结契约及断言。
真实破坏性测试仅使用专用实验会话。既有工作会话不用于制造故障。
不重新开展已经取消的新建项目任务；远程新建会话仅针对已登记项目。
共享状态与写入由协调者管理；代理只读核对主线和验收矩阵。

## 2026-11-08 历史进展

v0.2.37 的 Native HEALTH-001 已由独立只读代理审阅通过，真实 CLI 驱动
以 19 pass / 0 fail / 2 pending 封存并确认清理。空会话在上游普通列表缺席，
导致已缓存记录漏显；v0.2.38 保留未知原生位置的只读缓存，仍须重新取证。
生产版本未变，不以旧版本的目标环境证据签收新版本。

## 2026-11-09 完成记录

已验收并部署的是 v0.2.38，源码提交
`3ad85d2062a90a11c20c59ae1d7e55cf8056de57`，不是后续仅记录收尾的版本。
冻结31条需求、478项测试与原生/远程16项专项均已通过。
最终原汇总器实际退出0，37条记录全部pass，release_ready=true；
正式manifest及报告位于
`outputs/remote-parity-integration/release-assembly-0.2.38-final-v2/`。
Cicero独立内容批准绑定精确subject；旧失败、旧未签稿和不完整探测均保留。

本机与M1各两张专用卡完成真实拖拽、表单、导航、运行阶段和归档核对；
最终四卡均在实际主机归档列表中。当前批次36组转发/预转发审计未越过四目标。
原受限输入local5/M1 5保持耗尽，本轮收尾模型输入新增0。
测试插件、三个测试面板、24个准确登记的临时进程及专用socket已清理。

正式插件session_board、18767服务及Native实际入口均核对为同一v0.2.38候选；
正式MCP进程与资源内容也已核对。新配置为
`/home/herman/.config/codex-session-board/config-0.2.38.json`，
部署收据在 `outputs/remote-parity-integration/deployment-0.2.38/`。
旧库停机后有热回滚日志，在私有副本做SQLite标准恢复并通过完整性检查后迁移，
新库逻辑匹配恢复后的提交状态，原数据库及日志字节未变。
迁移带出的两张已归档M1测试缓存经同host新鲜归档读回后回填确认标记；
未重复上游归档，事务内其他会话和标题行未改变。

维护代价主要在连接身份、上游原生接口、并发令牌与升级迁移。
状态、标题和归档仍复用本地业务逻辑。旧userMessage重编号可能使旧表单保守失效；
不保证原生原子CAS、掉电或所有上游异常事件，未新增付费测试掩盖边界。
正式M1的发现、原生位置和运行连接均通过；bb macbook仍有66MB响应超16MB上限，
两台Hermes登记为停用，本次未对这些主机制造测试写入。
远程定时归档保持关闭，本机原有30分钟定时配置保留。
