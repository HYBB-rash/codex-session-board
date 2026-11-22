# Codex Enhancer 会话看板

先用随包提供的 `.sha256` 核对压缩归档。在解压目录使用 Node.js 22 或更新版本。

```bash
node dist/bin/codex-enhancer.js detect
node dist/bin/codex-enhancer.js install
node dist/bin/codex-enhancer.js launch
```

登录后从 Codex 侧栏打开「会话看板」。网页入口是 `http://127.0.0.1:18767/`。通过 Codex 自己的远程连接设置选择主机。状态列使用同名的 Codex 侧栏分区。

安装器只修改解压目录下的客户端副本。macOS Codex `26.1111.21308` 的框架内嵌 ASAR 完整性摘要，安装器会报告该构建无法加载修改过的启动入口。

将更新包解压后，把 `dist/` 和 `release.json` 放进初次安装目录的 `state/update-candidate/`，在看板点击「更新」，再完全退出并启动客户端副本。

```bash
node dist/bin/codex-enhancer.js status
node dist/bin/codex-enhancer.js rollback
node dist/bin/codex-enhancer.js uninstall
```

退出副本后再卸载。看板配置和运行数据在 `state/session-board/`。
