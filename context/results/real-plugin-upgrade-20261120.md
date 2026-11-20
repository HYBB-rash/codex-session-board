# 0.4.15 正式候选升级故障

owner: codex
execution_mode: isolated_write
allowed_write_set: scripts/, acceptance/, Docs/, context/results/real-plugin-upgrade-20261120.md
base_commit: 8a44008c926cdbbe6d7cc4114a8a2c54c1dbfe78
worktree_path: /home/herman/.codex/worktrees/plugin-update-button/notion-program
coordinator: PR 合并协调者

2026-11-20 22:43，旧版按钮已校验 Linux 候选 `0.4.16`，源码 `8a44008`，发布包提交 `3def989`。
原市场注册移除后，Codex 执行完整 Git clone，22:45 更新器在 `marketplace add` 超时。
临时目录的 178,302,347 字节 Git pack 在 22:50 才落盘，HEAD 仍停在默认主线。
现场 `0.4.15` 的原生缓存、成功回执和运行进程均保留，`0.4.16` 仅在已验证的发布包目录。
具体首报、原始日志和市场读回保存在本机 `~/.local/state/codex-session-board/real-upgrade-20261120/incident/`。

PR #29 分别执行过本地 Git fixture 的按钮升级及真实 GitHub 包下载。
前者触发原生安装、重载并读回版本，但本地小仓库掩盖了完整传输。后者没有执行原生注册。
前次提交门禁与隔离测试缺少更新器从正式 GitHub 取包直至宿主新版读回的一体证据。

隔离复现让旧版原生 `git clone` 超过缩短后的 1 秒测试时限，旧市场丢失且 `0.4.15` 缓存仍在。
修订版使 Codex 只从已校验浅克隆注册，保留正式来源和发布包提交。
使用真实 GitHub 包的隔离验证在 91.368 秒完成。注入市场添加超时后重新打开看板读回 `0.4.15` 及测试备注，重试后自动重载并从独立宿主读回 `0.4.16`。
实际安装后的隔离 MCP 看板也完成两次按钮升级，发起方是装入修订 updater 的 `0.4.15` 夹具，目标均为正式 `0.4.16`。
一次关闭页面后自动重载成功，一次按「已安装，待重载」提示重启隔离后台后成功。两次均读回原状态和备注。

验证入口为 `scripts/verify_plugin_upgrade.py`，参见 `Docs/DEPLOYMENT.md`。
本机证据目录 `~/.local/state/codex-session-board/real-upgrade-20261120/` 下的 `github-native-final.json`、`fixture-native-final.json` 和 `fixture-no-candidate-final.json` 分别记录正式来源隔离成功、本地快检成功和候选缺失。
`ui-auto/` 与 `ui-pending/` 保存隔离按钮、回执和截图。日用实例仍需在修复发布后独立取证。

2026-11-21，PR #31 在 `28eec63` 被退回。Codex 0.171.0 会去掉 GitHub HTTPS URL 的尾斜杠并补 `.git`。
原更新器用输入 URL 匹配 Git 重写，无 `.git` 时 Git 把后缀接到本地目录名上，有尾斜杠时重写未命中。
原生 Git 两次均退出 128。修复后，两种输入都注册校验过的 `3def989` 浅克隆。
两次隔离升级均验证了市场添加超时后恢复 `0.4.15`，重试后从独立宿主读回 `0.4.16` 和原有状态与备注。
本机记录在 `~/.local/state/codex-session-board/real-upgrade-20261121/`。运行方式见部署文档的升级验证报告。

第二轮复审在 `856b983` 发现注册缺失时旧缓存被清理、无 `RELEASE.json` 的本地市场无法升级、
回滚会安装或启用旧插件。隔离 Codex 0.171.0 复现三项问题。修复后从无注册的 `0.4.15`
注入安装后校验失败，核对缓存哈希和成功回执，重启宿主读回 `0.4.15`。
无清单的历史本地包 `0.2.59` 完成升级和失败恢复。未安装、已停用两种状态均验证了市场添加超时
和安装后校验失败，市场、缓存和启用状态与升级前相同。原生证据保存在本机记录的 `rollback/`。
