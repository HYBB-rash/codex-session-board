# socket 反复失效的长期方案报告

已核对两个自定义四小时清理器、实际 timer 与日志，隔离复现顶层目录 fuser 漏掉内部活 socket。
用户清理结束到目标首条 ENOENT 约十秒，支持其为首要触发因素，未冒充已取得历史删除者审计。
客户端源码确认 app-tools 没有运行期重绑，社区读取桥把活 authority 当作路径仍可用。
报告优先建议停用两套错误清理策略并修复 owner 生命周期，再独立验证公开分区 API 迁移。
已收独立只读审阅，补齐任意名称候选、timer 停用和不重启任务的验收条件。
报告位于 Docs/SOCKET-LIFECYCLE-REPORT-20261111.md，私有证据及复现脚本位于 outputs/socket-lifecycle-report-20261111/。
未修改外部脚本、timer、客户端安装包或生产状态，尚未实施修复。
