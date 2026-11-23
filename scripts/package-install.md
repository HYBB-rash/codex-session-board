# Codex Enhancer 会话看板

先用随包提供的 `.sha256` 核对压缩归档。在解压目录使用 Node.js 22 或更新版本。

## 默认副本

```bash
node dist/bin/codex-enhancer.js detect
node dist/bin/codex-enhancer.js install
node dist/bin/codex-enhancer.js launch
```

登录后从 Codex 侧栏打开「会话看板」。网页入口是 `http://127.0.0.1:18767/`。通过 Codex 自己的远程连接设置选择主机。状态列使用同名的 Codex 侧栏分区。

安装器只修改解压目录下的客户端副本。macOS arm64 的官方 Codex `26.1111.21308` 支持副本安装，使用本机 ad hoc 签名并保留 ASAR 完整性校验。安装使用普通用户权限。首次启动确认原生窗口和看板服务。启动失败时恢复副本的官方应用包并保留 `logs/` 下的诊断。

将更新包解压后，把 `dist/` 和 `release.json` 放进初次安装目录的 `state/update-candidate/`，在看板点击「更新」，再完全退出并启动客户端副本。

```bash
node dist/bin/codex-enhancer.js status
node dist/bin/codex-enhancer.js rollback
node dist/bin/codex-enhancer.js uninstall
```

退出副本后再卸载。看板配置和运行数据在 `state/session-board/`。

macOS 副本卸载会恢复官方签名与完整应用包。签名备份缺失或校验失败时，卸载停止并保留副本。macOS 当前提供副本安装。`--in-place` 仅适用于已核对的 Fedora x86_64 RPM。macOS 发布签名及公证仍待验。

每个 macOS 副本使用专用应用标识保存系统偏好。已有副本需退出后，在同一部署目录运行新版安装器的 `install`，以迁移应用标识并保留看板数据和独立配置。

## Fedora 原地安装

原地模式支持通过预检的 Fedora x86_64 Codex RPM。确认系统提供 `/usr/bin/node` 22 或更新版本，完全退出原 Codex，再从解压目录执行安装。

```bash
sudo /usr/bin/node dist/bin/codex-enhancer.js install --in-place
/usr/bin/node dist/bin/codex-enhancer.js status --in-place
```

从原 Codex 图标启动，打开侧栏的「会话看板」。回环网页为 `http://127.0.0.1:18768/`。看板沿用该窗口已有的项目、登录和远程连接。

更新、回退和卸载前先退出原 Codex。更新前核对新版压缩归档的 `.sha256`，再把解压目录传给 `update`。

```bash
sudo /usr/bin/node dist/bin/codex-enhancer.js update --in-place /path/to/extracted-candidate
sudo /usr/bin/node dist/bin/codex-enhancer.js rollback --in-place
sudo /usr/bin/node dist/bin/codex-enhancer.js uninstall --in-place
```

RPM 更新覆盖 Loader 时，已登记的用户服务会提示复查。卸载默认保留个人数据。系统卸载后，可用普通用户身份运行 `/usr/bin/node dist/bin/codex-enhancer.js purge-in-place-data` 清理当前用户的看板设置、备注和缓存。

默认私有路径为 `~/.local/state/codex-enhancer/in-place/`。已设置 `XDG_STATE_HOME` 时，使用 `$XDG_STATE_HOME/codex-enhancer/in-place/`。手动启动公共 CLI 或 MCP 时可用 `--state-dir` 选择该目录。

## 随包复验

在解压目录复跑随包测试和冻结契约时，无需安装构建依赖。模型对照需要 Python 3。

```bash
npm test
python3 -B acceptance/verify-typescript.py --model-only
node acceptance/check-candidate.mjs --open
```

逐项清单列出仍需真实宿主取证的验收。
