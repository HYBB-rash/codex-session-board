# 迁移交接

本包包含插件 0.2.5 的运行源码、插件与 marketplace 清单、离线资产、测试，
以及 acceptance-v1 的 31 条验收条件。它是当前行为的可迁移基线，
不是“所有目标环境已经验收”的证明。

## 接手后先做什么

1. 阅读 `acceptance/requirements.json` 和 `acceptance/README.md`。
2. 在隔离目录执行自动验收及上下文检查，保存本轮报告。
3. 选择目标架构。Python、SQLite、HTTP 桥及轮询属于当前实现，不是必须沿用的产品要求。
4. 迁移代码时保持需求编号，替换 `acceptance/bindings.json`、测试和适配器。
5. 在目标 Codex 客户端完成外部验收，提供对应实现指纹的证据。
6. 运行默认的完整验收，所有必需项目通过后才能报告开发完成。

当前执行基线是 Linux 和 Python 3.10 及以上。运行插件需要已有 Codex CLI 和客户端；
自动测试复用随包的 WebSocket 依赖，不连接真实工作会话，不需要 API key、Node.js 或 pip 安装。
原生界面及客户端深链接依赖目标客户端能力。Windows、远程主机和其他桌面变体
没有因此获得支持或通过验收。若目标是其他语言，验证器仍可使用 Python，
业务测试通过标准输入输出 JSON 协议接入目标语言的测试工具。

```bash
python3 scripts/verify_acceptance.py --scope automated --report /path/to/automatic.json
sh scripts/verify_context.sh
python3 scripts/verify_acceptance.py --evidence /path/to/evidence.json --report /path/to/final.json
```

默认完整验收因为缺少目标环境证据而返回 2 是预期的未完成结果。
不能改用只跑自动部分的命令来声称完整通过。

## 给开发 Agent 的任务文本

```text
以 acceptance/requirements.json 为验收基线，实现或迁移这个 Codex 会话看板。
先阅读 acceptance/README.md、MIGRATION.md 和项目上下文。

保持 31 个需求编号及其前提、动作、预期结果和验证边界。
保留待归档先于已完成的显示顺序，但允许人工跨列调整。
保留真实运行、阻塞和正常结束的自动状态规则；不得用助手自述推断完成。
不启用真实归档、自动发消息或启动任务。

为目标实现补充行为测试，更新 bindings 和 JSON 测试适配器。
测试本身必须调用目标实现；不要继续测试旧 Python 实现来给新实现签收。
新增测试加入映射；不删需求、不弱化断言、不跳过失败、不绕过契约锁。
需求需要改变时列明差异，取得用户确认后再更新基线。

先通过自动验收，再按每条外部验收步骤操作独立运行的目标实现。
真实界面必须在目标 Codex 客户端验证；缺少环境或证据时报告未验证。
证据绑定本轮需求和实现指纹，旧版本截图或测试报告不能直接复用。
只有默认完整验收退出 0、release_ready=true，且证据内容经审阅，
才能报告开发完成。提交报告、证据索引及仍存在的范围限制。
```

## 包的内容与校验

`PACKAGE-MANIFEST.json` 列出每个交付文件的 SHA256。
归档旁的 `.sha256` 文件校验整个归档。归档使用固定时间和相对路径，
同一份源码重复打包产生同一摘要。

本机实际配置、状态库、会话截图、运行日志、缓存和 node_modules 不打包。
项目上下文保留结构和决策，状态重置为迁移交接，不带开发过程的任务与日志。
没有将历史实测当作迁移后的通过证据。

重新打包：

```bash
python3 scripts/package_migration.py --output /path/to/codex-session-board.tar.gz
```

如需要安装运行，按照根 README 的插件安装步骤操作，并为目标机器配置外部数据目录、
客户端命令和桌面 socket。只做自动验收时不应安装插件或修改现有生产配置。
