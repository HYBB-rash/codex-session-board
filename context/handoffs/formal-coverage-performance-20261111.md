# 形式化验证补强与重放整合交接

owner: codex
status: awaiting_integrated_acceptance
execution_mode: isolated_write
worktree_path: /home/herman/.codex/worktrees/formal-coverage-performance/notion-program
branch: codex/formal-coverage-performance
coordinator: main-integration

用户要求先补齐业务验证，再优化重放；最新强调正确性第一，不能为速度缩小保证。
重放阶段封板后，整合复审发现了错误类别检查缺口；最终提交一并补强。
共享STATE、journal、任务归档与最终main由整合会话负责。

统一main基点为464108853c17381b9340489e6da826d7a86fe31c。
整合会话01a1ca26-dcc6-7602-a288-837ce5df3b47已同意继续逐提交接入：

| 提交 | 本地版本 | 内容 |
| --- | --- | --- |
| 6b02aa8 | 0.2.87 | 版本、返回值及归档连续检查 |
| 7097299 | 0.2.88 | 时间边界与真实关闭重开 |
| 34c9ad9 | 0.2.89 | 首次观察等号边界、失败前提、完整类型化快照 |
| 8093775 | 0.2.90 | 完整夹具隔离、磁盘schema复用、实际步数及连接标记 |
| 本记录所在来源提交 | 0.2.91 | 明确业务拒绝、传播技术错误、两个独立检查及最终文档 |

上述四个提交各自完整提交钩子及干净`./scripts/check`通过。
最后提交会随本记录交接；这些是分支开发版本，不是部署版本。
主分支合并时按父节点进位，保留历史及13文件/400行限制。

原生模型保持2,465状态、56事件、138,040边；草稿30边。
两套冻结重放各520代表路径、1,417实际步骤、40次关闭重开、40次查看。
真实原生重开涵盖pending、输入提醒、失败标题任务、归档、恢复及备注保留。
33项生产行为变异和11项验证设施变异均被捕获；不把编译失败算作捕获。
Backup通过初态等价实验但同条件更慢，没有引入；没有新的业务剪枝。
细节、边界及测量方法见Docs/FORMAL-VERIFICATION.md和对应结果记录。

| 单测试测量 | 旧方式 | 8093775样本 |
| --- | ---: | ---: |
| 原生138,040边 | 35.047秒 | 37.339秒 |
| 本地冻结重放 | 170.418秒 | 76.530秒 |
| 远端冻结重放 | 207.598秒 | 96.591秒 |

最后的错误分类补强保持全部状态和边；17项formal及Clippy通过。
未读分区、编辑、归档和草稿仅接受明确的业务错误，底层SQL/解码错误向上传播。
业务提示包装技术异常也不放行。新增两个未读拒绝测试，绑定AUTO-006。

合并注意保留main新增的section_model、section_fixture、sections及其独立测试。
验收绑定取实际测试的并集；新测试不能从映射中掉出。
冻结requirements.json、model.py、contract.lock.json没有修改。
本轮没有推送、部署或操作生产数据。

最终必须对合并后的实际HEAD执行完整verify_acceptance.py并审阅报告；
旧分支结果不能签收新指纹。缺少真实环境证据继续保持待验证。
只有完整验收退出0、release_ready=true且证据经审阅，才可宣称发布完成。
整合完成后由协调者更新共享上下文，再运行verify_context和干净的scripts/check。
