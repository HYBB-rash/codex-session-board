# E1 · 回复预览交付记录

- branch: codex/refactor-e1-vue-preview
- base_commit: e31cbe7ce7261f596168dda92e1569efc518ca58
- started_from: b859880165908a848a4982d2cfd9b722e9aeb912
- worktree: /Users/hongyong/.codex/worktrees/ae2e/codex-session-board
- evidence_root: /Users/hongyong/.codex/visualizations/2026/11/21/01a1fc61-077c-7320-8fc8-e48cb0d1758d/e1-evidence
- shared_context_owner: Herman / 01a1fc2d-af6f-7d50-9759-33835b51eaeb

## 实现边界

`assets/reply-preview.vue` 唯一持有正文、选中读取和阅读事件，模板组织布局。
`assets/reply-actions.tsx` 提供交互按钮，`assets/reply-preview.ts` 定义内部宿主接口。
宿主通过唯一 canRead 表达保存、拖动、新建和拖后抑制规则，所有组件读取入口先核对。
`index.html` 已删除原预览布局、replyStatus、updateReplyAvailability、
alignReplyToBoard、closeReply 和预览专用手工事件监听。
保留的 openReply 只查找最新会话并调用组件；卡片选中 aria 是尚未迁移卡片的接线。

复用原离线构建入口，生成 IIFE 写入共享 index.html 的指定生成区。
HTTP 与 MCP 使用同一产物，Vue 与 TSX 都进入严格类型检查及离线逐字节核对。
源码和许可证平放 assets，现有源码包与指纹覆盖，无新增运行时网络依赖。
31 条需求、契约锁和独立模型未改。未迁移 A 保存、D 映射、E2 卡片或编辑。

## 证据索引

以下文件位于 evidence_root，最终成功状态及源码绑定以报告内容为准：

- candidate-final/VALIDATION.json、automated.json：固定最终头的项目检查、全目标 Clippy 和全部自动验收。
- source-final/VALIDATION.json：源码包重新解包与指纹核对。
- offline-final.log：最终头 .vue/TSX 类型检查和离线逐字节重建。
- startup.json、startup.resource.html、packaged-browser.log：实际包启动、包内 stdio resources/read 和相同预览场景回放。
- REVIEW.md：最终准确头的独立只读复审。
- T3.md、guard-before.log、guard-after.log：等价改写、真实错误、类型错误和守卫回归的对照。
- validation/step-3.lease.json：首轮因守卫问题主动取消的原始清理回执，不计为成功。

本项认领的完整测试 ID 保存在活动任务卡，没有移除既有测试或降低断言。
真实 Codex 客户端未安装、未重载、未验收，不能以隔离浏览器或资源回放签收。
未更新 main、推送、发布或部署；协调者决定合入和共享任务收尾。
