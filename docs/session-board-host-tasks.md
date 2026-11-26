# Enhancer 主机任务状态验收与迁移

## 数据归属

看板和「全部任务」读取会话所属主机的任务记录。

Enhancer 从当前 Codex 窗口的本机或 SSH 连接解析目标主机的实际 `CODEX_HOME`，在同一 ChatGPT 账号下通过该目录找到同一存储。
两个独立的 Codex 数据目录各有自己的 `identity`，可以包含相同会话 ID。

| 内容 | 位置 |
| --- | --- |
| 状态、备注、待处理原因、输入与流转版本 | 所属主机的 `CODEX_HOME/enhancer-tasks-v1-<账号摘要>/` |
| 远端最后确认的展示数据 | 访问端的 `state/session-board/task-cache.json` |
| 旧备注、配置、排序偏好与归档账本 | 原安装的 `state/session-board/` |
| 会话、执行、项目及实际归档 | Codex 原有数据空间 |

目录要求由目标 App Server 的运行用户拥有，权限为 `0700`。
同目录内写入完整草稿后用硬链接发布版本，同一版本只接受一个提交。
变更通知触发刷新，后台轮询补读遗漏的变更。
归档意图单独占用同一输入版本的提交位置，原生归档结果需要另外核对。
状态和备注不会加入原生会话附件。

## 隔离迁移预演

1. 解压候选包，用其 `node dist/bin/codex-enhancer.js install` 创建副本。测试账号只使用明确指定的测试会话。
2. 确认源安装、待验证的 Codex 数据目录以及账号。关闭各写入端，用只读备份保留原安装的 `state/session-board/`，以及各主机已有的 `enhancer-tasks-v1-<账号摘要>/`。目录尚未创建时记录这一事实。
3. 在隔离副本的 `state/session-board/config.json` 中设置 `"taskState": "migration"`。保留该文件的其他字段，继续用 `writableThreadIds` 限定可写的测试会话。
4. 启动副本，在包根目录取得预览。预览包含备注和原生归属，保存到访问受限的目录。

```bash
umask 077
node dist/bin/codex-enhancer.js task-migration preview > migration-preview.json
```

5. 按主机核对候选总数、`import`、`unchanged`、`review`、`conflict` 和 `missingLegacyRecords`。对每台旧访问端分别核对预览，保存出现分歧的原值。核对 `oldArchiveBatches` 中 `archived`、`active`、`unknown` 与旧回执状态。
6. 用该份预览的 `token` 提交。记录 `imported` 与 `pending`，再次预览并实际读回。源快照变化、目标已产生新版本或归档情况不明的项目，由操作者核对后再决定。

```bash
node dist/bin/codex-enhancer.js task-migration apply '<已核对预览的 token>' > migration-result.json
node dist/bin/codex-enhancer.js task-migration preview > migration-readback.json
```

| 旧记录 | 预演处理 |
| --- | --- |
| 五个业务分区，归属可确认 | 导入相应业务状态、备注和有效待处理原因 |
| 无原生分区 | 保留未归列及备注 |
| 待判断、旧已完成、无法识别的分区、待处理原因缺失或失配 | 保留原值供核对，仍显示在未归列中 |
| 旧端与已存在的目标记录不同 | 两值保留在预览中，目标版本继续生效 |
| 断线或未验证来源 | 列入预览缺口 |
| 可能已发起的旧归档 | 显示原生核对结果，继续保留旧账本供人工核对 |

隔离验收中，将配置切到 `"taskState": "host"` 后重启，再在两个视图中确认读回。
正式切换由 Herman 核对多端预览和误差清单后决定。
排序偏好沿用各安装自己的账本，两个视图使用同一排序规则。

## 回退与保留新增修改

暂停各端写入后，先备份当前主机任务目录，再移除隔离安装配置中的 `taskState` 并重启。
旧看板重新读取原生分区与原有账本，主机任务目录保留切换后写入的所有已提交版本。
需要把切换后的修改继续用于旧看板时，通过备份中每个会话最新的版本逐条核对并录回。
恢复主机任务模式时应继续使用原主机目录，核对旧模式期间产生的分歧。
保留旧快照和切换后的快照，原生已归档的会话由 Codex 原生归档功能单独处理。

## 验收边界

本版的远程任务管理仅通过 Codex 已建立的 SSH 连接启用。
看板与「全部任务」会提示 Remote Control 暂不支持，选择目标主机的 SSH 连接后即可管理其任务。
`auth.json`、真实任务记录及含备注的迁移预览属于本地测试数据，候选包无需携带它们。
缺少归档回执、且原生列表尚无法确认时，任务保留待核对。同一输入版本不会自动重发。
