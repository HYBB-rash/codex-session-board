# C1 远程归档的数据归属

owner: codex/refactor-c1-archive-ownership
execution_mode: isolated_write
base_commit: b859880165908a848a4982d2cfd9b722e9aeb912
worktree_path: /Users/hongyong/.codex/worktrees/c1f3/codex-session-board
coordinator: Herman / 01a1fc2d-af6f-7d50-9759-33835b51eaeb

交付分支为 codex/refactor-c1-archive-ownership。普通版本保留 0.4.19。
首件测试检查点为 1ae672fd133a79779e506c4207be864adb3306c1，
实现检查点为 3079cc7baf7405bb55c119a40f4ac23fb053257b。
两次提交分别为 4 文件 250 行、5 文件 203 行，均经暂存审阅、
./scripts/check --pre-commit 和 Agent 签名提交钩子。

## 维护者需要查看的调用

src/app/archive_batches.rs 继续拥有根库中的批次名单、提交结果及恢复入口。
实际主机操作进入 src/app/archive_batches/host.rs 的 HostArchive。
构造处分别传入 root_batches 和 target_host。
submit 用目标主机的既有连接读取绑定来源，将 pending 与来源记入根库，
然后在同一原生写连接上执行原有输入、分区保护和归档。
check 只使用 CatalogReader，读回并复核来源，向目标主机库确认会话归档，
向根库记录批次 verified 结果。来源不符保持 pending。
没有新增恢复入口、转发回原旧实现的旁路或跨库原子提交承诺。

## 隔离首件及故障证据

tests/native_authority/remote_archive_batches.rs 使用现有真实 HTTP 服务、
独立临时根库、mac/other 两个主机库及受控原生代理。
本机、mac、other 同时保留相同会话 UUID。
用例完整 ID 登记在本任务卡和 acceptance/bindings.json。

- 正常回执仅确认 mac 的会话，根库记录 archived，重开仍不重放原生归档。
- 写入前在 thread/read 断点直接读库，根库已 pending，原生写入为 0。
- 原生成功且回执丢失后根库仍 pending，mac 原生已归档，写入累计 1。
- 重开批次入口仍显示 pending，重复提交不写。只读核对后根库 verified，
  仅 mac 库确认归档，本机及 other 不变，写入累计仍为 1。
- 原生新输入导致 newInput 拒绝，核对、重开和重复提交均不写，累计为 0。
- 切换原生账号保持 pending，恢复原绑定账号后才确认，其他主机仍不变。

首件在未修改业务实现的基线通过，三项 14.14 秒。
原始证据保留于 /private/tmp/c1-archive-first-fixture-unrestricted.log。
改动后的 16 项 native_authority 归档及 11 项 archive_flow 通过，
见 /private/tmp/c1-archive-refactor-tests.log。
增加正常回执后四项远程场景通过，20.22 秒，
见 /private/tmp/c1-archive-complete-fixture.log。
写入前断点的独立补验通过，5.86 秒，
见 /private/tmp/c1-archive-prewrite-observation.log。
上述阶段的相关 lib/test Clippy 均通过。
最初沙箱拒绝监听 Unix socket 的失败保留在 /private/tmp/c1-archive-first-fixture.log，
没有用后续成功冒称原失败没有发生。

## 最终核对入口及边界

最终提交的完整 SHA、基线、验证命令、退出码和独立只读审阅回执，
由交付会话保存到源码外 /private/tmp/c1-archive-final-head.json。
请以该回执及最终会话回复为准，不以较早检查点替代最终提交的验证。
sh scripts/verify_context.sh 已通过；共享状态、登记、日志及任务归档留给协调者。
31 条需求、独立参考模型及锁均未改动，没有降低或跳过原有故障断言。
仅验证隔离 fixture，未操作日用 Codex、真实远程服务或会话。
没有推进 main、推送、部署，也没有宣称完整 31 条目标环境验收完成。
