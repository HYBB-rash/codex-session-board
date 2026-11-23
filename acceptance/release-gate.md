# 发布验收门槛

从候选源码构建后，向门槛提供同版本的 Linux x86_64 和 macOS arm64 发行归档。`--identify` 读取每个归档的 SHA-256、平台、版本和运行文件清单，并核对归档的源码与本地构建。它可用于准备实测记录。

每份归档旁边保留同名 `.sha256` 文件。交付断言核对两份归档的离线资源、测试、迁移说明、数据与缓存排除、内部运行文件清单和外部 SHA-256。报告按平台在 `archives` 的 `deliveryChecks` 列出内容或校验缺口。

```bash
npm run build
python3 -B acceptance/release_gate.py --identify \
  --archive linux-x86_64=/path/to/linux.tar.gz \
  --archive macos-arm64=/path/to/macos.tar.gz
```

实测人员在对应的宿主安装该归档，按 `requirements.json` 中各条 `then` 的顺序保存观察和原始附件。`releases.json` 的 `runs` 对每个 `mixed` 或 `live` 需求和平台各记一条，字段如下。

| 字段 | 内容 |
| --- | --- |
| `schemaVersion` | 根对象版本 `1` |
| `contract`、`sourceSha256` | 从 `--identify` 输出复制 `contract` 以及 `candidate.sourceSha256` |
| `runs[].requirement`、`platform` | 冻结需求 ID，宿主平台 `linux-x86_64` 或 `macos-arm64` |
| `runs[].version`、`archiveSha256`、`runtimeSha256` | 从 `--identify` 对应平台的 `archives` 项复制 |
| `runs[].host` | `os` 与 `arch` 为 `linux` / `x86_64` 或 `darwin` / `arm64`，另填 `machine` 与实际 `codexBuild` |
| `runs[].operator`、`observedAt` | 验收人和带时区的 ISO 8601 观察时间，距验收最多 30 天 |
| `runs[].assertions[]` | `index` 从 1 开始，`status` 为 `pass`、`fail`、`error` 或 `skip`，`observation` 记录实测现象 |
| `runs[].assertions[].artifacts[]` | 每个通过断言的非空原始附件，包含相对同目录的 `path` 和小写 `sha256` |

记录和附件保存在候选源码目录树之外。验收人核对附件确实记录了实现中的操作和读回。脚本核对归档、附件、测试、时间和实测范围。

门槛使用独立 Chrome 配置运行随包 MCP 界面测试。可通过 `BOARD_TEST_BROWSER` 指定 Chrome 或 Chromium 可执行文件。单独复验界面执行 `node --test acceptance/board-interaction.test.mjs`。

```bash
python3 -B acceptance/release_gate.py \
  --archive linux-x86_64=/path/to/linux.tar.gz \
  --archive macos-arm64=/path/to/macos.tar.gz \
  --evidence /path/to/releases.json
```

报告逐项列出冻结断言和自动、现场结果。`gate-bindings.json` 按 `then` 的原顺序绑定自动断言，空数组代表仍缺覆盖。补上测试后可引用 `candidate-map.json` 中登记的证据 ID，或冻结模型适配器的断言 ID。脚本重新执行已绑定检查，跳过和失败都会阻止通过。`--model-only` 只执行独立模型对照。

仅 `releaseReady: true` 且退出 `0` 表示通过完整门槛。退出 `1` 表示失败或输入不合规，退出 `2` 表示仍有待验项目。现有历史记录可供查证，正式发布需要当前候选两平台的现场凭证。

```bash
python3 -B -m unittest discover -s acceptance -p test_release_gate.py
```
