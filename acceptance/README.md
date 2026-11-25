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
