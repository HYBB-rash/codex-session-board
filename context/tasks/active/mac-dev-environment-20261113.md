# Mac 双任务隔离开发环境

owner: codex
execution_mode: isolated_write
allowed_write_set: AGENTS.md, examples/, scripts/dev, scripts/dev_environment/, scripts/check-dev-isolation.mjs, scripts/browser-client.mjs, scripts/check-ui-layout.mjs, tests/, acceptance/test_dev_environment.py, acceptance/bindings.json, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, Docs/MAC-DEVELOPMENT.md, Docs/PARALLEL-DEVELOPMENT.md, context/
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

## 完成条件

- 两份 Mac 工作树独立运行真实看板业务代码，并可分别保存状态和备注。
- 停止、重启一份不影响另一份；任务数据持久保存，误配真实端点时拒绝启动。
- 正式插件配置、安装身份和运行进程前后不变；外部证据保存在源码之外。
- 必需项目检查通过；完整业务缺少的目标环境证据仍保持待验证。
