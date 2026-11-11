# Mac 开发迁移与插件发布

owner: codex
execution_mode: isolated_write
allowed_write_set: src/, tests/, scripts/, Docs/, .github/, .agents/plugins/, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, acceptance/bindings.json, context/
base_commit: f9c68da06de85bc2f7e739f5df9e638d8f480d3e
worktree_path: /home/herman/文档/codex-session-board-migration-20261111
coordinator: codex-mac-migration

## 授权与边界

用户于 2026-11-11 授权按已讨论方案迁移完整开发上下文到 MacBook，
并实际打通构建、发布和更新。目标为 hongyong 的 MacBook Pro M1。
Fedora 原检出仍有独立形式化任务进行；本任务使用独立 clone，不暂存或提交其改动。
保全全部 Git 引用、其他工作区 HEAD、工作区差异、项目上下文和开发证据。
机器凭据、实时 socket、生产配置和数据库不作为可发布插件内容。

## 完成条件

- Mac 收到可校验的完整历史、开发上下文及各工作区快照。
- Mac 原生构建运行，服务身份与停止保护保持有效。
- Linux 与 macOS 候选插件可从固定入口安装、更新并读回实际运行版本。
- 私有远端保存源码和可审阅发布流程，稳定发布保留完整验收门槛。
- 更新保留配置和数据，提供固定版本回退与失败报告。
- 目标环境缺证时保留候选，不宣称稳定验收完成。

## 现场

- 2026-11-11：SSH 已验证 MacBookPro、hongyong、arm64、zsh。
- Mac 既有 Rust 1.78.0，正在升级；已有 Git、Python、Codex 与独立 Agent 签名器。
- Fedora GitHub 已登录 HYBB-rash；Mac GitHub 登录失效，禁止复制 Fedora 凭据。
- 原检出无远端；创建私有源码仓库并以 Git bundle 完成首次迁移。
- 已从实际部署回执核对 0.2.92 源码 f9c68da，并采用其完整后继作为实现基线。
  原主检出的形式化任务仍独立保存，不以旧主检出覆盖已部署功能。
- 合并用户委托的跳转故障修复：来源聊天 01a1ca2b-5173-7383-ad1f-806fb9c6f253。
  服务可能继承已删除的 worktree cwd；客户端子进程必须设置稳定工作目录。
  安装、更新与回退须同时验证路径、cwd、实际版本和真实跳转。
  旧正式 data_dir 位于历史 release 内，禁止随 release 清理删除。
