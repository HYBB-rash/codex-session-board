# Mac 并行开发与 Linux 即时交付设计

owner: codex
execution_mode: serial_write
allowed_write_set: Docs/PARALLEL-DEVELOPMENT.md, Docs/MAC-DEVELOPMENT.md, context/STATE.yaml, context/tasks/active/parallel-development-design-20261113.md, context/tasks/completed/parallel-development-design-20261113.md, context/journal/2026-11-13-parallel-development-design.md, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json
base_commit: 06c4719
worktree_path: /home/herman/文档/notion-program
coordinator: codex

## 目标与授权

用户要求先设计完整流程，复用现有工具，让 Mac 上的独立任务开箱即用，
完成的改动尽快进入 Linux 正在使用的插件；Linux 特有问题允许在本机修复。
本轮交付仓库内设计文档，不实施工具、不连接远端、不推送或部署，
不修改现行版本规则、验收标准或真实配置数据。

## 工作安排

- 主代理统一编写设计、维护共享 context、验证并提交。
- 两个只读子代理分别审查运行环境隔离、Git 集成与版本规则。
- 明确现有能力、待实现接口、迁移顺序、异常恢复和可验证的完成条件。

## 设计交付

- 启动时 main 干净，基线为 06c4719。
- `Docs/PARALLEL-DEVELOPMENT.md` 定义环境隔离、宿主租约、主线所有权、
  当前版本规则下逐提交重放、后续版本切换、固定源码/产物交付与 Linux 实际读回。
- 区分 candidate 交付和完整验收，保留 31 条及原生/远程证据门槛。
- 记录 CI 取消交付和更新回执误判风险；给出七步实施及故障/并发签收场景。
- 在 `Docs/MAC-DEVELOPMENT.md` 增加设计入口，现行运行命令和规则未改。
- 两个只读子代理核对隔离资源、协议 fixture、签名和版本重排，主代理统一审阅。
- 只完成设计文档；没有实现新命令、推送、连接远端或安装插件。

## 验证

- 收尾执行 context 校验、提交前检查、提交钩子及普通项目检查，结果见本次会话记录。
- 设计涉及的端到端场景属于后续实施验收，本轮不声称已经跑通。
