# Mac 开发迁移与插件发布

owner: codex
execution_mode: isolated_write
allowed_write_set: src/, tests/, scripts/, Docs/, README.md, requirements-delivery.txt, .github/, .agents/plugins/, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, acceptance/bindings.json, acceptance/test_delivery.py, acceptance/test_update_delivery.py, acceptance/test_remote_release.py, context/
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
- Mac Rust 已从 1.78.0 升级到 1.99.1，安装 Clippy、rustfmt，缓存锁定依赖。
- Mac GitHub CLI 登录失效；已有 SSH 身份现已通过 GitHub 真实取包，无凭据跨机复制。
- 私有仓库 HYBB-rash/codex-session-board 已创建，首次完整历史与上下文由 bundle 迁移。
- 已从实际部署回执核对 0.2.92 源码 f9c68da，并采用其完整后继作为实现基线。
  原主检出的形式化任务仍独立保存，不以旧主检出覆盖已部署功能。
- 合并用户委托的跳转故障修复：来源聊天 01a1ca2b-5173-7383-ad1f-806fb9c6f253。
  服务可能继承已删除的 worktree cwd；客户端子进程必须设置稳定工作目录。
  安装、更新与回退须同时验证路径、cwd、实际版本和真实跳转。
  旧正式 data_dir 位于历史 release 内，禁止随 release 清理删除。

## 已验证与续作

- 732MB 迁移材料、五工作区快照、133 份项目聊天及额外清理保全材料已抵达 Mac；
  归档 SHA 和逐文件 SHA 已核对。聊天作为历史材料保留，不混入实时会话索引。
- 原部署 f9c68da 与统一 main 4641088 分步合入，保留双方历史和验收映射。
  其他正在开发的形式化提交仅归档保全，不冒充已交付 main。
- Mac 原生 socket、身份核验停止、旧构建停止和删除 cwd 后导航已有测试。
- GitHub 0.2.98 Mac 候选实际安装、服务身份和 MCP 重载均成功；
  Linux 隔离安装与固定 Git SHA 回退已成功，原生产服务未切换。
- 用户提供的新 Agent 签名器已检查，直接调用系统 ssh-keygen；
  远端 Git 认证也独立通过，未登记新 GitHub 密钥。
- Mac 既有委托凭据可直接用于仓库专用 HTTPS 拉取，真实 fetch 已过；
  token 无写权限，pushurl 保留已有 SSH，源码自主推送仍待验证。
- Linux 0.3.8 自动范围 543 项通过。Mac Python 路径别名、共享 deadline
  测试调度与云端磁盘耗时继续按实际失败修正，不降低断言或跳过测试。
- Linux 0.3.9 完整范围退出 2：543 自动项全过，17 需求通过、14 待真实环境证据。
  无审阅证据的稳定发布实际拒绝。Mac 文稿目录额外副本已保全，
  开发检出迁至 Developer；原目录保留，不忽略指纹变化来签收。
- 最终源码推送、CI、升级回退及配置数据保留结果继续写入 handoff 指定的运行回执。
- 完整业务验收仍须真实目标环境证据，不能以候选交付或自动检查代签。

续作入口：`context/handoffs/mac-development-release-20261111.md`。

## 2026-11-12 验证进度

- 形式化封板 f8d716d 已分七步合入并保留全部父节点；冻结需求、模型及锁未改。
- Developer 干净检出、六工作区快照和旧文稿副本均保全；最新聊天检查点为 140 份，137 份基线全部保留并在 Mac 校验。
- 0.3.20 双平台 CI 完整成功。Fedora 与 Mac 独立解包源码均为 558 项自动通过，
  完整范围退出 2、17 条通过、14 条待真实环境证据，两边源码指纹一致。
- Linux 已完成 0.3.20 → 0.2.98 → 0.3.20，实际服务/MCP 读回、配置、WAL 备注及备份完整性均通过。
- Mac 首次升级已运行 0.3.20，全局技能刷新连续超时使交付保持 pending。
  定向配置重载及目标 MCP 读回约 1.5 秒成功；交付脚本移除无关的全局扫描，严格版本与工具核验保留。
- 本次修订将从 Mac 使用既有 Agent 签名器实际提交和推送；最终全链路结果继续以运行回执为准。
- 修订控制器已在 Mac 完成 0.3.20 → 0.2.98 → 0.3.20，实际 Git 快照、全部缓存哈希、
  服务与 MCP 版本、配置和备份均通过；原失败回执继续保留。提交后的新版本仍须 CI 与真包闭环。

## 剩余授权与最终回执

- Mac 已在真实仓库创建 f9daae5，作者和提交者均为 Cindy Agent。
  在 SSH_AUTH_SOCK=/nonexistent 下签名及本机验签成功，普通 Git 全局配置哈希未变。
- 真实推送失败；既有 SSH agent 能列出身份，但 GitHub 密钥签名请求被拒绝或通信失败。
  同一签名提交通过 Fedora 已授权连接原样发布，没有转发 agent 或复制凭据。
- f9daae5 的 0.3.21 Linux、macOS CI 全部成功，561 项自动检查通过。
  收尾版本只更新四份版本清单与开发接力；最终真包闭环仍以运行回执为准。
- 私有仓库 HTTPS 读取可用，现存 token 写入和默认分支设置返回 403。
  用户需恢复 Mac 现有 GitHub SSH 授权，并将仓库默认分支改成 main。
  此前 dry-run 成功不能替代本次真实推送结果。
- 保持任务活动和阻塞记录，不以候选交付签收完整业务。后续从 handoff 与最终回执继续。
