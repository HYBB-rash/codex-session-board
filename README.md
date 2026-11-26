# Codex Enhancer 会话看板

从 Codex 的 Enhancer 侧栏打开会话看板。看板复用当前窗口的本机与远程连接，状态列对应 Codex 原生侧栏分区。默认安装独立副本。Fedora 也可以选择原地安装。

## 安装和打开

在 Linux x86_64 上试用当前工作区时，先用 Node.js 22 或更新版本运行 `npm ci` 安装依赖，再执行 `npm run install:copy`。命令打包、校验并启动独立副本，默认安装在 `~/.local/share/codex-enhancer/preview/`，也会遵循 `XDG_DATA_HOME`。更新前请完全退出测试副本，再执行同一条命令。副本的桌面配置和看板数据独立保存，日用 Codex 继续通过原图标打开。

如果日用 Codex 将登录缓存写在 `CODEX_HOME/auth.json`，且副本还没有自己的缓存，该命令会让副本引用同一份登录缓存。日用 `CODEX_HOME` 默认为 `~/.codex`。两个窗口使用同一份登录凭据，账号退出或切换可能影响另一个窗口。

本轮交付 `0.3.33`。Fedora 44 x86_64 的 Codex `26.1111.71704` 已通过副本和原地模式的安装、启动、更新、回退和卸载验收。macOS arm64 另有对照用副本候选，剩余验收暂缓。 [下载与校验文件](docs/session-board-typescript-acceptance.md#033-候选) · [macOS 暂缓项](docs/session-board-typescript-acceptance.md#本轮交付范围)

在 Linux 候选包所在目录校验归档，再进入解压目录运行 Node.js 22 或更新版本。

```bash
sha256sum -c session-board-0.3.33-linux-x86_64.tar.gz.sha256
tar -xzf session-board-0.3.33-linux-x86_64.tar.gz
cd session-board-0.3.33-linux-x86_64
node dist/bin/codex-enhancer.js detect
node dist/bin/codex-enhancer.js install
node dist/bin/codex-enhancer.js launch
```

macOS 副本候选在官方签名的 Codex `26.1111.21308` 上验证，采用本机 ad hoc 签名并保留 Electron ASAR 完整性校验。独立副本写入当前用户可写的解压目录，安装无需管理员权限或 Developer ID。首次启动确认原生窗口和看板服务。若启动失败，安装器恢复副本的官方原包，诊断保存在 `logs/`。

```zsh
shasum -a 256 -c session-board-0.3.33-macos-arm64.tar.gz.sha256
tar -xzf session-board-0.3.33-macos-arm64.tar.gz
cd session-board-0.3.33-macos-arm64
node dist/bin/codex-enhancer.js detect
node dist/bin/codex-enhancer.js install
node dist/bin/codex-enhancer.js launch
```

安装器把客户端复制到本目录的 `state/`，只给副本的启动入口加 Loader。登录后，从侧栏打开「会话看板」。通过 Codex 的远程连接设置连接主机。状态列使用同名的原生侧栏分区，缺少分区时先在原生侧栏登记。远程项目尚未在该桌面登记目录时，「新建会话」会打开 Codex 的项目登记对话框，由用户确认后打开输入页。

安装前可用 `CODEX_ENHANCER_SOURCE` 指定 Codex 客户端所在目录，启动时可用 `CODEX_CLI_PATH` 指定客户端使用的 Codex 命令路径。Linux 副本默认使用客户端随包命令，在 `XDG_RUNTIME_DIR` 下使用独立的桌面 socket。用 `CODEX_ENHANCER_BRIDGE_SOCKET` 可指定该副本的 socket 路径。相对命令和 socket 路径以调用启动器的目录为准。

## 更新和回退

先核对候选归档的 `.sha256`，将解压后的 `release.json` 和 `dist/` 放入安装目录的 `state/update-candidate/`。在已安装看板里点击「更新」，完全退出并重新启动 Enhancer 副本，再打开看板核对版本。也可以从安装目录执行这些命令。

```bash
node dist/bin/codex-enhancer.js update /path/to/extracted-candidate
node dist/bin/codex-enhancer.js rollback
node dist/bin/codex-enhancer.js status
```

更新和回退切换已校验的运行时目录，配置和记录保存在安装目录的 `state/session-board/`。退出副本后，执行 `node dist/bin/codex-enhancer.js uninstall` 恢复副本的原始启动归档。

macOS 副本卸载会恢复官方签名与完整的应用包。安装器保存并验证原始签名备份。备份缺失或校验失败时，卸载停止并保留现有副本。

新版 macOS 安装器会为每个独立副本分配专用应用标识，用于隔离系统偏好。已有副本在停止运行后，在同一部署目录运行新版安装器的 `install`，原有看板数据和独立配置会保留。

## Fedora 原地安装

从本地源码部署时，在项目根目录执行 `npm run install:in-place`。命令会打包当前工作区、校验归档，再根据安装状态执行首次安装或更新。首次安装前请退出 Codex，部署成功后从原图标启动或重启。

原地模式要求 Fedora x86_64、由 RPM 管理的 `/opt/codex-desktop`，以及 `/usr/bin/node` 22 或更新版本。当前源码支持指纹匹配的 Codex `26.1111.71704` 和 `26.1120.50957` RPM 构建。安装器还使用 Fedora 的 `rpm`、`flock`、`getfattr` 及桌面用户服务。选择与客户端构建兼容且带原地安装协议的 Linux 候选，校验 `.sha256` 后解压。退出原 Codex 的所有进程，再在解压目录执行。

适配新的 Codex RPM 时，先运行 [Fedora RPM 本机会话验收](acceptance/README.md#fedora-rpm-兼容性)。

```bash
sudo /usr/bin/node dist/bin/codex-enhancer.js install --in-place
/usr/bin/node dist/bin/codex-enhancer.js status --in-place
```

从原 Codex 图标启动，沿用该窗口已有的登录、项目和远程连接。系统归档备份、看板运行时和当前版本指针保存在 `/var/lib/codex-enhancer/`，归系统管理员所有。每个用户的看板数据保存在 `$XDG_STATE_HOME/codex-enhancer/in-place/`，缺省为 `~/.local/state/codex-enhancer/in-place/`。原地网页使用 `http://127.0.0.1:18768/`，公共 CLI 和 MCP 可用 `--state-dir` 指向原地数据目录。Fedora 44 已在隔离 RPM 环境验证沿用专用测试登录、项目和双端连接，完成原生移卡、原生 MCP App 和卸载复验。现场覆盖见[验收记录](docs/session-board-typescript-acceptance.md)。

校验新版归档后，将 `update` 参数指向新版解压目录。更新和回退在下次启动 Codex 时生效。卸载前请退出 Codex。

```bash
sudo /usr/bin/node dist/bin/codex-enhancer.js update --in-place /path/to/extracted-candidate
sudo /usr/bin/node dist/bin/codex-enhancer.js rollback --in-place
sudo /usr/bin/node dist/bin/codex-enhancer.js uninstall --in-place
```

客户端构建不兼容时，原地 Loader 会停用看板。RPM 覆盖 Loader 后，独立用户服务在登录或检测到应用目录变化时复查，并提示安装兼容版或清理旧登记。卸载时校验备份与当前归档，按实际 RPM 状态恢复原件或保留升级后的文件。遇到未知改动时保留恢复材料并报告冲突。

卸载默认保留每个用户的看板私有数据。要删除当前用户的备注、设置和本地缓存，先完成系统卸载，再以该普通用户运行 `/usr/bin/node dist/bin/codex-enhancer.js purge-in-place-data`。此前在 Codex 原生会话上完成的操作保留。

## 网页、PWA 和 CLI

Enhancer 副本运行时，网页看板在 `http://127.0.0.1:18767/`。可以从浏览器的应用安装入口安装。网页、MCP 和 CLI 的会话操作由运行中的 Codex 窗口执行。连接断开时，缓存只用于展示。

```bash
node dist/bin/codex-session-board.js serve
node dist/bin/codex-session-board.js scan
node dist/bin/codex-session-board.js check-sections
node dist/bin/codex-session-board.js mcp
```

MCP 配置见 [.mcp.json](.mcp.json)。从另一目录调用时可以指定 `--state-dir`，或把 `CODEX_ENHANCER_STATE_DIR` 指向 Enhancer 的 `state/`。`stop` 核对实例后关闭网页服务。重新启动 Enhancer 副本可恢复服务。

在 Codex 桌面端，从 Enhancer 侧栏的「会话看板」打开页面。

## 设置与迁移

在 `state/session-board/config.json` 设置 `mode`（`four`、`five` 或 `legacy`）和 `syncTitles`。默认显示进行中、待审阅、待处理、已阻塞和未归列区，标题同步默认关闭。卡片标题右侧的「归档会话」执行单条 Codex 归档，顶部「已归档会话」用于查找、查看和恢复由看板归档的会话。旧配置中的定时间隔不生效。

升级时先保留原生「待归档」分区，再从当前 Enhancer 的 CLI 生成只读迁移清单。

```bash
codex-enhancer archive-migration preview
codex-enhancer archive-migration apply <预览返回的token>
```

`apply` 确认迁移已启用。再次运行 `preview` 可查看剩余卡片和逐卡回执。迁移按宿主把仍在旧分区的活动会话移到「待审阅」，离线宿主接入后继续。分区重名、缺失或会话已经移动时，在清单或连接详情中报告冲突。历史批次只参与读回。有明确移动回执和位置版本的会话才能自动回退。需要回退时先关闭旧版两个定时归档设置，再重新生成预览，用新 token 执行 `codex-enhancer archive-migration rollback <token>`。回退会停止后续自动迁移。位置已变化或回执丢失后仅由读回确认的会话，请依据清单人工核对。

使用「待处理」前，在 Codex 原生侧栏创建一个同名分区。看板通过当前 Codex 连接移卡，由 Codex 在首次需要时补建目标主机的分区。从看板移入时填写搁置原因，从原生侧栏直接移入后可以在看板补填。回到「待审阅」时手动移卡。

`pageSize` 和 `maxPages` 默认均为 100，达到分页上限时会报告扫描未完成。`historyPerPoll` 默认为 20，控制每轮每台主机的历史补读数量。

```bash
node dist/bin/codex-session-board.js migrate --source-config /path/to/legacy-config.json
node dist/bin/codex-session-board.js migrate --rollback MIGRATION_ID
```

迁移对旧主库及主机分库建立私有 SQLite 快照。以迁移回执中的 `id` 回退，回退前会验证快照完整性。要把旧记录映射到新看板，在 `state/session-board/legacy-binding.json` 写入同一 Codex 身份对应的 `accountId`、`userId`，以及 `hosts` 对象，将旧主机 ID 映射到新窗口确认的主机 ID。重启后读入。未绑定分库留在快照中，已有新记录和显式设置优先。

## 构建和复验

```bash
npm ci
npm run typecheck
npm test
npm run lint
python3 acceptance/verify-typescript.py --model-only
node acceptance/check-candidate.mjs --open
bash scripts/package.sh 0.3.23 state/acceptance/packages linux-x86_64
bash scripts/package.sh 0.3.23 state/acceptance/packages macos-arm64
```

查看 [Fedora 原生移卡检查点](docs/session-board-native-checkpoint.md)、[冻结模型对照](acceptance/typescript-model.md)、[逐项验收清单](acceptance/candidate-map.json)和[统一发布门槛](acceptance/release-gate.md)。
