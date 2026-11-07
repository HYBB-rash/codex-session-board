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
