# TypeScript 候选验收记录

`candidate-map.json` 按固定上游提交绑定 31 条需求、18 个共享操作和 7 条 CLI 命令。`fixture` 表示已有覆盖该项的隔离夹具，`partial` 表示已有证据但仍有待核验断言，`pending` 表示尚无绑定的候选证据。18 个操作均执行了 HTTP 与 MCP 的合成回执对照，各操作仍需核对表中记录的业务断言。

```sh
node acceptance/check-candidate.mjs --open
npm run build
node --test dist/compat/entrypoints.test.js dist/migration/legacy.test.js \
  dist/installer/paths.test.js dist/installer/release/store.test.js dist/runtime/registry.test.js
```

第一条命令检查冻结文件及操作登记的哈希、清单完整性、证据路径和测试名，并打印待核验项。真实宿主证据要从受测候选安装件复验并记录。Fedora 的原生移卡证据适用于记录中的检查点。

`live/` 存放工作区的真机验收工具。用 `python3 -B acceptance/live/board-accept.py --help` 查看 Mac 副本验收入口，运行记录和日志写入 `state/acceptance/cli-runs/`。

## Fedora RPM 兼容性

适配新的 Codex RPM 时，先用打包后的看板运行本机会话验收，通过后再安装到日用客户端。准备一个与目标宿主同版本的 Fedora Podman 测试镜像，镜像需要有非 root 用户 `tester`、Codex RPM 的运行依赖、Xvfb、D-Bus、Node.js 22、`curl`、`jq` 和原地安装所用的系统工具。

将目标 RPM、看板归档及同名 `.sha256` 放在同一目录。以下命令使用当前工作区的验收脚本，在断网容器及全新的临时用户目录中安装并启动指定版本。

```bash
podman run --rm --pull=never --network=none --user 0:0 \
  --security-opt label=disable --security-opt seccomp=unconfined \
  --volume "$PWD/acceptance/live:/suite:ro" \
  --volume "$ARTIFACTS:/artifacts:ro" \
  --entrypoint /usr/bin/bash "$FEDORA_TEST_IMAGE" \
  /suite/fedora-rpm-smoke.sh "/artifacts/$RPM_FILE" "$CODEX_BUILD" \
  "/artifacts/$BOARD_ARCHIVE" "$BOARD_VERSION"
```

脚本给隔离窗口提供合成账号上下文和本机连接通知。会话列表由候选包的 preload 向实际 RPM 的 App Server 请求，通过看板的 `/api/sessions` 核对结果。脚本核对运行中的两个版本，并要求本机会话扫描完成、有当次启动后的成功时间且无读取错误。空白配置返回零条会话可以通过。窗口虽能打开但请求超时、只读到离线快照或一直未能读取都会以非零状态退出。真实登录、远程会话和写入操作仍由相应现场用例验证。

## 候选与现场证据

```sh
python3 -B scripts/board-candidate.py build --ref HEAD --version 0.3.32
python3 -B acceptance/live/board-accept.py doctor --host macbook --scenario native-turns
python3 -B acceptance/live/board-accept.py collect --candidate 0.3.31 --gate
```

`board-candidate build` 从指定 Git 提交构建并测试，把两份归档、SHA-256、测试日志和 `candidate.json` 写入 `state/acceptance/candidates/<version>/`。同一版本目录只能发布一次。`collect` 默认查找该清单，也可用 `--manifest` 指定。已有 `run` 支持同一候选清单，在当前 Mac 原生轮次夹具支持的候选版本上运行。

`doctor` 读取已有隔离 Mac 副本。要检查原生远程移卡，使用 `--scenario native-move --target-host-id <原生主机 ID> --thread-id <专用卡 ID>`。完整检查需要该副本已经运行并开放回环调试端口。可用 `--profile` 指定隔离副本，`--remote-python` 指定远端解释器，`--ssh-control` 复用已有 SSH 控制连接。

`run` 和 `doctor` 支持 `--temporary-key`。它为 Fedora 发起的验收连接签发一次性文件密钥，可配合 `--ssh-control` 复用一次初始授权。 [批量验收示例](../scripts/gui-test.md#临时-ssh-文件密钥)

`collect` 从 Mac `native-turns` 回执生成逐项凭证，并默认导入现场目录中的 `live-<version>.json` 历史凭证。可用 `--import-evidence` 改为导入一份或多份指定凭证，附件路径相对于现场记录目录。输出 `collected-<version>.json`，列出已通过、失败、缺证和冲突。`--gate` 从已核对的 Linux 候选归档解包运行正式发布门槛，并另存 `collected-gate-<version>.json`。退出 `2` 表示仍有待验项，退出 `1` 表示证据或测试失败。
