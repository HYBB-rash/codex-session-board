# PR21 真实合并与耗时记录

owner: codex
execution_mode: isolated_write
allowed_write_set: 机械合并、冻结版本、本任务记录、context 状态和队列源码外证据
base_commit: c397839004bb69e60ab6cd334fa2616faf5ef2a5
worktree_path: /Users/hongyong/.codex/worktrees/four-pr-rehearsal/codex-session-board
coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
status: completed

最终bb06b572951f747892ba1b68bfdd49096f043fdd/0.4.3已合入远端main，
macOS1060项、Linux1071项及准确作者头0182407的PR CI通过。
首次ready至main实测1小时18分12.036秒，包含两次失败和原作者两轮回修。
原队列作者closeout完成，外部timer/service已移除；原会话未请求归档，
作者源码、分支、工作树与冻结检查器保留。本任务临时候选分支和工作树已清理。
完整测量、限制和证据索引见context/results/pr21-integration-20261117.md。
发布独立运行，没有安装或重载日用插件。

用户授权按现有流程合并刚提交的真实 PR，并记录实际耗时。
目标 PR21，初次观察准确头681f68c189bbb0d77458253d0b0a537acee859a3，
初次观察时间2026-11-17T09:14:33.463022Z，当时仍为草稿、CI运行中。
就绪起点以GitHub ready_for_review实际事件为准，准备和收尾另列。
队列复用作者Linux x86_64冻结检查器c397839及同版本工具。
独立候选工作树和队列记录单独创建，不修改作者源码或检查器。
发布由main自动流水线继续；不安装、重载日用插件或代签客户端验收。

## 执行过程

原始准确头独立审阅在作者rollout的2026-11-17T09:05:58.961Z消息中核实。
队列09:18:29.827761Z初始化，机械合并566c095，无冲突；
冻结43f3ae6d6eaa05bf790314e9c2707c138f1cfa65/0.4.1。
作者PR CI44053046425于09:17:30Z通过；作者CLI巡检后，
GitHub ready_for_review为09:20:09Z。候选CI44054543905于09:19:37Z创建，
双平台约09:19:58Z开跑。协调端GitHub查询于09:24:05Z出错一次，
在原输出目录恢复，未重跑CI或改源码，原错误保存。

Linux候选业务回归976通过1失败，失败为remote::cache的
explicit_connection_overrides_startup_preference_without_changing_desktop_configuration，
报错“会话只有目录或历史缓存记录，当前不可修改”。
原失败报告artifact14316141060已下载，main未更新。
具体反馈ad2a790f-7e6e-46b4-a8a0-0a696dd38627已发布到PR评论6944099160。
原作者会话01a1e83e-9729-7570-affe-cd8147882431收到真实turn
01a1e8d6-9d48-7b73-932b-3f9bad2e4162，并于09:29:29.447423Z领取，
事件623747e1-d0ae-4b33-a02c-a857c5b39b9e，当前修复第1轮。
保留Mac结果，等待作者交回准确新头的审阅和验证，不由协调者修业务代码。
源码外证据位于当前会话visualizations目录下pr21-integration-20261117。

macOS首轮965通过1性能失败，cold=0.891s/warm=0.256s，补充反馈评论6944256511。
首轮候选09:34:47Z正式失败，原队列反馈babd1d1f-47f9-5234-98d3-acabcfde2f02保留。
作者修复Linux夹具和扫描竞争，独立审阅明确复核
54d361e1f12c41aa32d2b34d501d97447af57860，无阻断项。
新submit834项通过，237.241秒，于09:48:51.914364Z交回；
09:51:07Z完成原事件回执，仍为一轮修复。复现证据32份附件摘要已核对。
原队列requeue/attempt已重新接纳，冻结新候选
83479439025a62fcdceb7795f2279001158b140c/0.4.2。
CI44059266181于09:50:31Z创建，双平台09:50:53Z开跑；
新头PR CI44059013781同时运行，准确头仍须通过后才推进main。
执行中的详细记录见context/results/pr21-integration-20261117.md。

第二次候选CI44059266181于10:00:43Z失败，Linux通过，
Mac完整automated1059通过1失败，check-host-filter.mjs:136的提示断言失败。
先前缓存和warm_guard用例均通过，不代表历史性能故障根因已修复。
具体反馈ca0f2b45-1cab-4bfe-b385-185bfca3ad81/评论6944866192已发布，
作者第二轮事件184a3498-27a7-4d40-b90b-94541872205d，main仍为c397839。
第二轮已受控复现相同UI断言并提交0182407b7914fc12f3748a4ee24e13e5efe2538a，
6项界面测试及绑定该完整头的独立审阅通过，正在运行冻结检查器submit。
新头审阅保存在远端源码外review-ui-0182407b7914.json及对应原始回执。
新submit834项通过，237.526秒，10:24:45.829656Z交回；
第二轮回执评论6945362430，已核对9份受控复现附件摘要。
原队列再次requeue/attempt后冻结bb06b572951f747892ba1b68bfdd49096f043fdd/0.4.3。
第三次候选CI44064889855于10:26:44Z创建，准确PR头CI44064593026同步运行。
第三轮启动时仍等待两端及准确PR头通过；最终结果见本卡开头。
