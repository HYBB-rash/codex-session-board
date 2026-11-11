# 最终整合续接

owner: codex
task: context/tasks/active/remote-final-integration-20261111.md
date: 2026-11-11
status: completing_runtime_pwa_and_gate_integration

用户授权仅操作专用验收会话，完整验收后直接部署，不再请求部署许可。
本树从2f63a5a分区实现建立，分支codex/remote-final-integration；
88已合并Main be7d079记录，89又合并S5571ff4的清理/生成预算修正及恢复基础。
89完整钩子、check/context和8恢复回归通过；90正在补全持续告警、observer防护、
另外3恢复回归与PWA安装按钮/host.error，尚待验证提交，不当作最终候选。
旧树remote-restore-acceptance在f8dad19记录85，其check和context已通过。
旧83身份、部分真实实测、失败和独立review保存在旧树outputs/remote-restore-20261110。
原cache83路径已消失，当前同名槽为另一任务88；binary83-retained.json记录了同SHA原构建包。
不恢复或覆盖该共享槽，最终入口用本会话独立命名空间。

门禁工作者在remote-final-gate独立树从88接入S预检/C九反例并修SHA/报告覆盖漏洞；
其自己的89提交允许与本树89并存，协调者在本树90之后合并、统一版本并重新冻结。
已有full CLI探针复现5次私有附件覆盖及2次缺SHA被接受；假夹具通过不签业务。
静态原集合并集526为下限，新增回归相应绑定。保持冻结31条及锁，不降低断言。
完整串行适配器仅外层预算改1800，内部600不变；上一轮串行Rust测试累计约912秒。
创建并行工作树前再次确认协调树干净、记录base；写任务各自worktree及branch。
工作者仅改其分配文件及唯一结果记录，不改协调树或任何真实会话。

本机A/B仍由state-section-release-20261111/local-run-86-01占用，未释放前不写。
远端A/B已由旧83测试恢复原名并归档；没有新增真实远端模型轮次。
刚重测M1只读SSH成功，但桌面read_thread仍host unavailable；原生菜单点击35秒超时，
未确认设置打开，不推测输入权限原因；已请求用户在设置连接里重连。
Linux CUA不支持实际drag，不能合成事件签原生UI。取消的新建项目任务不重启。
最终冻结后新取完整31条及16项、同FP/binary/config、实际UI和内容review。
release_ready=true并退出0后部署，保留稳定dataDir、用户归档偏好及关闭的Hermes。
