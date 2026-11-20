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
