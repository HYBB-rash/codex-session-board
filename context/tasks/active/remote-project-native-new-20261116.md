# 在目标主机原生项目中新建会话

owner: codex
execution_mode: coordinated_parallel
allowed_write_set: src/, tests/, scripts/native_new/, scripts/check-native-new.mjs, scripts/check-native-target*.mjs, scripts/check-remote-catalog.mjs, scripts/check-app-bridge.mjs, index.html, acceptance/bindings.json, acceptance/test_native_new.py, acceptance/test_native_target.py, Docs/REMOTE-NATIVE-NEW.md, 本任务文件
base_commit: d7c1717428e0fb56312afa74f82db942d7813d3a
worktree_path: /Users/hongyong/.codex/worktrees/remote-project-native-new/codex-session-board
coordinator: codex

用户授权按照已验证方案实现、验证并提交 PR，使用项目 pr-task 技能。
不修改 Codex 后台，不安装或重载日用插件和客户端。
作者分支为 codex/remote-project-native-new。
冻结检查器为 /Users/hongyong/.codex/worktrees/remote-project-pr-checker/codex-session-board，版本固定为上述基线。

目标后台项目直接使用真实 ID；旧项目仅在明确新建时按原生规则完整导入，
使用原旧 ID 作为幂等键，不迁移历史会话，不补写旧映射文件。
原生输入页以独立草稿身份保存主机、项目和目录，用户发送后由原生流程创建。
目标草稿不使用缺少项目身份的旧预热缓存，不写错误的旧归属。
同一操作重试保持身份；项目或来源变化、删除及丢回执如实处理。

客户端适配作者工作树：/Users/hongyong/.codex/worktrees/remote-project-native-client/codex-session-board。
项目准备作者工作树：/Users/hongyong/.codex/worktrees/remote-project-native-prepare/codex-session-board。
两者只提交各自范围；协调者审阅后组合，不并发改同一文件。
独立专用 PR 会话负责私有状态和巡检，不把原调查会话登记自动归档。

验证包括原生转换、RPC 边界、请求恢复、浏览器交互、当前客户端实际函数、
隔离真实后台与本地模型替身首轮，以及最终源码 PR 门禁和准确头 CI。
自动验证不代签日用客户端部署或完整 31 条目标环境验收。

2026-11-16 组合验证记录：
- 已通过 65 项目录/新建浏览器断言，包括刷新重试、明确重开、HTTP 拒绝、删除和离线。
- App 入口经已核验的隔离 SSH/RPC fixture 完成规范项目导航与旧项目打开失败重试；
  同一操作复用 canonical，不重复导入，也不发送 thread/start 或 turn/start。
- 原生项目准备 16 项回归覆盖完整 metadata、分页期间变化、迁移刚完成、
  最后 canonical 读回、已删除和 foreign backing；全部目标 RPC 经过 App::connection。
- 实际 Mac 候选主模块摘要 b5666e2944e61eb1ab14f32efed7c128259a756d5504737809ee66b2b87ab6d1。
  从生成候选抽取 _D 和 xz.prepareConversation/prepareStart，连接隔离的 0.169.0 后台；
  两个同目录项目分别完成本地 SSE 模型替身首轮，SQLite 的 project_id/cwd 与请求一致，
  通用预热不可消费，外部模型请求为零，临时进程和目录已清理。
- 可重复探针为 scripts/check-native-target-start.mjs，由候选源函数检查器提供输入。
- 当前 Fedora 导出源候选主模块摘要 aab8c55c69179bec359d23c069e12a62169dc0d2c5b543b87459d294156a0dd8。
  同样在 Mac 的隔离 0.169.0 后台完成两个项目的首轮和 SQLite 核对；此项不是 Fedora 在线验收。
- 首轮已经绑定 UUID 后，账户 A→B→A、roots 改变后恢复，仍保持操作失效并拒绝重试发送。
- 隔离后台的 4 次时序探针确认 turn/start 刚接受时 projectId 暂为空；
  对同一 UUID 做有限读回，其他非空项目立即拒绝，超时保持“尚未确认”，不重建。
  Mac/Fedora 两份源候选都已在首轮接受时立即执行此核对，随后首轮和 SQLite 校验通过。
- 真实日用客户端候选安装、多窗口 GUI、远程在线首轮与完整目标环境验收仍待部署阶段。
