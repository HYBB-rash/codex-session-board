# TypeScript 候选验收记录

`candidate-map.json` 按固定上游提交绑定 31 条需求、18 个共享操作和 7 条 CLI 命令。`fixture` 表示已有覆盖该项的隔离夹具，`partial` 表示已有证据但仍有待核验断言，`pending` 表示尚无绑定的候选证据。18 个操作均执行了 HTTP 与 MCP 的合成回执对照，各操作仍需核对表中记录的业务断言。

```sh
node acceptance/check-candidate.mjs --open
npm run build
node --test dist/compat/entrypoints.test.js dist/migration/legacy.test.js \
  dist/installer/paths.test.js dist/installer/releases.test.js dist/runtime/registry.test.js
```

第一条命令检查冻结文件及操作登记的哈希、清单完整性、证据路径和测试名，并打印待核验项。真实宿主证据要从受测候选安装件复验并记录。Fedora 的原生移卡证据适用于记录中的检查点。
