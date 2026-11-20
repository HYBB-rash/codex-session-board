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
