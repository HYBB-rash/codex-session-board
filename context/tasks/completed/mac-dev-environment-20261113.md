# Mac 双任务隔离开发环境

owner: codex
execution_mode: isolated_write
allowed_write_set: AGENTS.md, examples/, scripts/dev, scripts/dev_environment/, scripts/check-dev-isolation.mjs, scripts/browser-client.mjs, scripts/check-ui-layout.mjs, scripts/package_migration.py, tests/, acceptance/test_dev_environment.py, acceptance/test_delivery.py, acceptance/bindings.json, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, Docs/MAC-DEVELOPMENT.md, Docs/PARALLEL-DEVELOPMENT.md, context/
base_commit: 82b9355
worktree_path: /home/herman/.codex/worktrees/mac-dev-environment/notion-program
coordinator: codex

## 授权与目标

用户授权实施第一阶段，在 Mac 准备可直接启动的开发环境，
真实验证两个任务同时独立运行，且不影响 Linux 正在使用的插件。
允许项目代码、Mac 开发工具和隔离工作树准备；不发布、安装或重载正式插件。
不改变版本、签名和完整验收规则，不复制机器凭据及生产数据库。

## 工作安排

- 主代理串行修改、验证和提交，维护共享 context。
- 两个只读子代理核对 fixture 协议和进程/配置隔离边界。
- 浏览器验证脚本由独立 `dev-isolation-browser` 工作树的 worker 实现，不修改共享文件或提交。
- 分步交付开发 fixture、统一 setup/start/stop/status、真实 Mac 双任务与浏览器取证。
- Mac 基线：main 06c4719，干净检出，arm64；Rust、Python 3.12、Node、Chrome、
  Codex 和 agent-git 已通过既有 SSH 连接确认存在。
- 本机基线：main 82b9355，干净；Linux 和 Mac 正式安装仅采集只读身份及配置摘要作前后对照。
- 开工后发现主检出有独立运行时恢复任务，已将本任务改动单独迁至 Codex 工作树，
  对方版本、数据库恢复改动及任务记录保留；后续不写共享主检出或其部署。
- 原生依赖 fixture 已通过独立协议/隔离测试及提交钩子，提交 2aaa3ac。
- 两份 Mac 工作树已分别完成约 45 秒首次构建，Mac fixture 单元测试通过。
- setup 配置边界已提交 07aa02f；本机真实服务启动、重复 setup/start、
  停止及重复停止已通过。正在补 Mac 双实例运行、浏览器写入及正式插件保护取证。
- 进程组守护与配置边界已提交 cd26cf3；真实忽略 SIGINT 的子进程清理测试通过。
  本机三种畸形控制消息均未打断服务，停止后确认受管进程组没有存活后代。
- Mac 首次双启动因接收 socket 继承非阻塞模式而拒绝就绪，日志保留于 pilot 证据。
  显式恢复接收连接的阻塞模式，补延迟帧回归；同时修正 Mac 临时目录别名比较，
  不降低原测试断言。浏览器脚本已在独立工作树完成身份防误接和私有 Chrome 检查。

## 完成条件

- 两份 Mac 工作树独立运行真实看板业务代码，并可分别保存状态和备注。
- 停止、重启一份不影响另一份；任务数据持久保存，误配真实端点时拒绝启动。
- 正式插件配置、安装身份和运行进程前后不变；外部证据保存在源码之外。
- 必需项目检查通过；完整业务缺少的目标环境证据仍保持待验证。

## 第一阶段结果

开发入口、Rust 原生依赖 fixture 和私有浏览器验证已实现。
两份 Mac 工作树独立构建、同时运行；真实拖拽及备注保存、刷新、导航记录、
停止 A 后 B 写入、A 重启持久性、fixture 崩溃隔离和恢复、运行中幂等操作均通过。
最终提交还将按同一外部流程复验，结果以 `final-lifecycle.json` 为准。
工作树为 `/Users/hongyong/Developer/codex-session-board-dev-{a,b}-20261113`；
私有根分别为 `~/.csb-dev/05773aea0e1e` 和 `~/.csb-dev/dda8c2b48a54`。

两端证据目录为 `~/.local/state/codex-session-board/development/mac-dev-environment-20261113`。
`pilot-*` 保留 Mac 非阻塞继承问题；`verified-browser` 保留 Chrome 子进程退出时序失败；
`verified-fixed-*` 是修正清理后的真实通过记录，浏览器检查器摘要单独登记。
Mac 正式插件在该验证窗口保持 0.3.38、PID 65822，配置、安装和缓存摘要前后相同。
另一个已授权交付将 Linux 从 0.3.36、Mac 从 0.3.34 更新至 0.3.38；
本任务未安装或重载正式插件。Linux 在早期本机测试前后未连上正式 HTTP，
最终复验开始时已可读取，PID 2255986；不能宣称整段实施期间正式入口全程可用。
完整 31 条业务验收仍按最终报告处理，fixture 不补签真实目标环境证据。
Codex 自动环境动作、串行集成及 Linux 即时候选交付留在后续阶段。
最终 0.3.43 的故障注入抓到清理时控制连接关闭与 status 查询竞态；
已保留失败报告并补真实 Unix socket 断开回归。该提交上的旧验收已明确取消，
不用其结果签收新实现；修正后在最终源码重新执行完整验收和 Mac 双任务流程。
0.3.44 的完整门禁为 611 项自动通过、1 项交付失败：源码白名单未包含 dev 入口和
三个环境模块。已保留该报告，扩充明确白名单并加强原迁移包测试，不降低指纹等价断言。
