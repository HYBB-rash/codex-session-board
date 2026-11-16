# PR19 真实业务合并计时

owner: codex
execution_mode: isolated_write
allowed_write_set: 机械合并、候选版本、本任务上下文；业务与冲突修复退回作者
base_commit: 6b985e0cd7defc9dea9a0b4c891b273b3260b363
worktree_path: /Users/hongyong/.codex/worktrees/four-pr-rehearsal/codex-session-board
coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
status: completed

最终结果：PR19已进入远端main，插件源码a34cb075811a417be63cc69aa3cf8285ef05d763，
版本0.3.99；macOS1046/Linux1057项全过，原作者头62d54db完整保留。
2026-11-16T17:06:51Z就绪到18:04:42.595865Z主线确认，
共57分51.596秒，包含两次GitHub容量故障的30分03秒空排队。
18:06:41Z读回自动发布pending，合并没有等待发布；未安装或重载日用插件。
作者任务complete、rounds=0，外部systemd巡检单元已核对归属并删除；
自动归档未登记，记录not_requested，作者源码、工作树及原会话保留。
详细计时及边界见context/results/pr19-integration-20261117.md。
以下保留阶段性执行状态，阶段中的“等待”不是最终状态。

用户授权使用现有新流程合并新的真实PR，测量就绪到main完成的实际时间。
PR19最终作者头62d54db894590b743832b2055cf9857159baa75b，
GitHub ready_for_review时间2026-11-16T17:06:51Z，就绪评论17:06:53Z；
首次读取准确头的证据文件时间17:08:30Z。
不重放旧PR，不把发布、安装、重载或人工验收纳入合并完成条件。
作者已提交Linux冻结检查器9e2f64e的1057项报告及两个独立审阅记录；
协调者核对原始证据，使用相同冻结检查器和既有pr-queue/integrate。
PR16仍是草稿，不为计时强行合并。

源码外证据：
/Users/hongyong/.codex/visualizations/2026/11/13/01a1d38b-1ef8-7392-ba78-caa7935c5ab4/pr19-integration-20261117/

计时保留认证准备、检查器平台切换、候选、CI排队和执行，不重置失败起点。
main自动发布单独读取状态；不操作用户日用插件或客户端。

队列在Fedora独立工作树执行：
/home/herman/.codex/worktrees/pr19-integration-20261117/notion-program。
源码外原始队列：
/home/herman/.local/share/codex-session-board/pr19-integration-20261117/queue。
17:16:23.874758Z进入队列，机械合并fc174b5，无冲突；
冻结候选a34cb075811a417be63cc69aa3cf8285ef05d763，版本0.3.99。
候选CI43916778874于17:17:17Z创建；
Linux于17:25:52Z一次通过，执行8分19秒。
首次跨机认证以及从作者原会话提取两个准确头审阅回执的准备时间保留。

首轮macOS在17:32:32Z被取消，15分02秒没有分配runner或执行步骤。
GitHub原始注释明确hosted runner未接走任务和macOS arm64容量不足，
不是业务测试失败；Candidate gate如实失败，主线未变。
原运行、原始注释与队列failed-run快照保留。
17:33:56Z同一run的attempt2只重试失败任务，Linux成功任务复用；
在PR补充基础设施归因，作者无须改业务源码。
等待真实新attempt通过后，再用原retry-candidate恢复同一候选，计时不重置。

attempt2同样在17:48:57Z因容量不足取消，15分01秒未执行步骤；
两次空排队共30分03秒，两个原始GitHub注释均已保存。
attempt3的macOS job133493016437在17:49:49Z创建、17:49:53Z取得runner，
正在执行完整验证；成功后已接上原retry-candidate自动继续，不等模型手工轮询再推进。
原队列作者修复请求以绑定旧评论全文SHA256的resolved回执解除，明确组合仍未通过、
main未更新；没有用该回执替代CI。作者17:37:51Z巡检仍watching、rounds=0，
没有因基础设施失败开启业务修复轮次。
Linux原始不可变报告已下载：1057项全过，规则18.341秒、
全目标Clippy35.937秒、完整自动测试384.802秒，库和native_authority仍两线程。
