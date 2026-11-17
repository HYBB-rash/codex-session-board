# 2026-11-17 项目操作准入修复

用户要求修复无需求及技术依据的限制，同时保留身份核验和数据保护。
源码分步提交至 30b910a，完成记录见
`context/tasks/completed/project-action-boundaries-20261117.md`。

修复固定版本准入、全目录告警连带阻塞、无关旧数据耦合、固定客户端品牌、
分区名称、装饰枚举和重复固定超时。复审追加修复首次校准名称限制及
缓存分区绑定冲突漏检，负向身份、删除、并发和读回断言保留。

固定提交上的 Linux PR 组合验证 992 项全过，源码包及插件隔离启动通过。
证据保存在
`/home/herman/.local/share/codex-project-action-boundaries-20261117/validation-final`；
原失败另存 `validation`，没有以重跑覆盖原始证据。
开发版本及共享 STATE 保持不变；没有推送或部署，真实客户端验收仍独立。
