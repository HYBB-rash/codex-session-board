# 实现来源与已吸收的故障案例

2026-11-16 查阅源码及测试，固定以下版本。借鉴状态转换、幂等和故障案例，
没有引入整个外部编排框架或复制其实现：

- EveryInc/compound-engineering-plugin：
  https://github.com/EveryInc/compound-engineering-plugin/tree/ee46dde75a40e297bd8eac4055a410a771d3ac1d
  重点是 ce-babysit-pr 的 pr-snapshot 及 snapshot.test.ts。
- OrchestratorInc/agent-orchestrator：
  https://github.com/OrchestratorInc/agent-orchestrator/tree/b5f4911e4c00f57fa79ea4b51f35da9926825bbb
  重点是 manager_test.go、provider_test.go、reactions.go。
- OpenAI Codex：
  https://github.com/openai/codex/tree/a394ff9cf66e9c17258077e8f8295178ca0c2025
  核对 queue 重试、新 client ID、原生 thread/archive 和子会话行为。
  未发布审阅的历史修复：
  https://github.com/openai/codex/commit/a770e5b8470d3320eb53a56a286ea4a0a70a1f59

| 源码/测试暴露的问题 | 本项目处理 |
| --- | --- |
| 第二页读取失败被当成没有反馈 | 全量分页，任何页失败拒绝整个快照；检查后重新确认 head/base |
| 尚未发布的 review/comment 触发修改 | 同时查 review 状态及行内评论父 review，PENDING 不消费 |
| 作者与审阅者共用 GitHub 账号 | 用显式信任账号和结构化标记，不按“作者自己”一律排除 |
| 读到反馈就记成完成，崩溃后丢任务 | 发现、投递、claim、complete 分开；完成回复也须读回 |
| 编辑旧评论漏处理，机器人回复无限循环 | 正文参与事件 ID；只接受可信反馈标记，自己的结果标记不触发 |
| 同 head 重跑检查无法区分，旧通过被复用 | head、check run ID、完成时间组成失败键；源码和检查证据重新核验 |
| 403/限流被当成空 PR 列表，后台凭据不可用 | 分类退避/暂停；安装前用真实后台环境验证写入和标签权限 |
| PR 结束就删除资源，打断作者或子会话 | 先确认原生状态，再归档读回，独立 finalizer 卸载自己的定时器 |
| 重启脚本就刷新修复预算 | 初始化持久化绝对期限和累计次数，只有显式恢复能扩展 |

上述经验也有互相冲突的实现选择：CE 对编辑过的评论保留正文变化，
AO 有以稳定评论 ID 去重的路径。本项目限定反馈来源并排除自身回执，保留正文变化。
不能因为外部项目已有某个 helper，就把它当作完整保证；本项目另加未发布父审阅、
分页错误、未知投递、原生子树和调度器清理的回归。

复用本项目既有 `validate --scope pr`、PR 验证证据核验、GitHub 凭据入口、
`pr_queue_transport` 的端点身份检查、固定 client ID 和丢响应对账。
新增部分承担作者状态机、GitHub 快照、macOS 纯代码调度和项目 skill。
没有复用 `codex queue` 作为盲重试工具：每次调用会生成新 ID，响应不确定时会重复入队。
现有原生运输可以让同一事件一直复用稳定 ID，并区分发送、原生轮次和处理完成。

自动回归位于 `acceptance/test_pr_task_*.py`，真实 CLI 子进程使用本地 GitHub fixture；
`acceptance/pr_task_rehearsal.py --output /absolute/new-directory` 运行真实临时 launchd
服务和独立 finalizer，GitHub 与会话仍隔离，不发送真实 PR 或模型请求。
fixture 结果不代替真实作者 PR 的端到端验收。原生归档缺少原子条件的限制见
[PR-TASK.md](PR-TASK.md)，没有用测试结果掩盖这一边界。
