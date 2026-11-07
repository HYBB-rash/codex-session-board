# 修复完整用户输入与摘要缓存不一致

owner: codex
execution_mode: serial_write
allowed_write_set: src/rpc.rs, src/store.rs, tests/archive_flow.rs, tests/observer_contract.rs, acceptance/bindings.json, index.html, Docs/ARCHIVING.md, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/, outputs/, bin/, 当前日常看板及已登记插件副本
base_commit: 49c3e53
worktree_path: /home/herman/文档/notion-program
coordinator: codex

用户在部署后的真实一键归档中报告两条会话被新输入校验拒绝。
只读对比证明 summary 仅包含该轮第一条用户消息，而 full 分别包含两条、三条；
轮次 ID 和历史结束时间未变，本地缓存一直保留不完整的摘要消息集合。
修复读取侧，使用 full 后立即提取 ID 与执行元数据，不保存正文。
保留归档对未知或新增输入的严格校验；旧摘要补读后不得伪造人工已核对。
仅在待复核的待归档卡片上提供显式确认入口，复用状态版本及输入版本校验。
先完成可验证修复并提交，再按已授权部署工作流更新日常看板。
不归档或替用户确认这两条真实会话，不推送，不降低冻结验收。
诊断见 outputs/archive-input-diagnostic-20261107/diagnostic.json。

0.1.68 源码改为统一 full 读取，在内存提取元数据后丢弃正文。
归档回归测试覆盖旧缓存补读、未确认仍拒绝、过期确认拒绝、显式确认后正常归档。
11 项归档测试、26 项观察测试和 RPC 隐私投影测试通过。
Node VM 验证确认只调用 /api/state、保留备注并提交双版本，冲突时保留待复核且不归档。
保留现有格式，cargo fmt --check 暴露的是 src/rpc.rs 和既有 native_authority 测试中的历史格式差异，未做无关重排。
修复提交 3d9d20a，版本 0.1.68；完整提交钩子及提交后 ./scripts/check 通过。
隔离 Chrome 验证先显式确认，再以更新后的版本执行批量归档，确认动作本身没有归档请求。
按延续的部署授权准备 0.1.69，保留原服务数据、人工确认与归档标记。

已部署 0.1.69，日常程序、marketplace、缓存与运行进程 SHA256 一致，
客户端插件及 MCP 均读回 0.1.69。配置、两份 MCP 环境注入和端口不变。
127 条既有记录的备注、人工选择、输入确认及归档标记全部保留。
真实只读核对，两条目标缓存已分别包含完整的两条、三条用户消息，
同时保持 needs_review=1，等待用户显式确认，没有伪造已核对或执行真实归档。
真实 Chrome 两张目标卡片均有“确认待归档”，通用调整状态按钮为零；
浏览器错误与写请求为零，已审阅窄屏截图。
新构建 301 项自动验收通过，完整门禁退出 2，14 项既有外部证据待补，release_ready=false。
部署与数据保留证据见 outputs/archive-input-fix-deployment-20261107/，
目录权限 0700。用户需重新打开看板，核对后确认这两张卡片，再执行归档。
