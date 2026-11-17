# PR 合并手动交接与恢复：本地交付

本地实现及隔离验证完成，真实 GitHub 演练由用户之后指定对象。
没有执行真实 PR 评论、合并、远端推送、定时任务变更或日用插件操作。

基线 b66656a63e81581ef1947c4223b74fb76915980d，开发准备起点为
2026-11-17T11:23:56Z。独立分支 codex/merge-manual-rounds-20261117，
工作树 /Users/hongyong/.codex/worktrees/merge-manual-rounds-20261117。
最终验证源码 28c31f0f0f51ddfb00933ddc1e4e75da9d68e7b7，版本保持 0.4.3。
最后的任务归档只改 context；交付提交及最终检查由源码外 COMPLETION.json 记录。

## 行为及复用

- integrate 默认只查询一次 CI，未完成正常返回 waiting-ci，固定已有 run 并记录各 attempt。
  resume 继续原候选，不重复推送、冻结或提升；同一输出目录有互斥锁。
- pr-queue 增加按仓库和 PR 定位及 resume，复用 Git 公共目录和作者工具的原子保存、锁。
  GitHub 是 PR、CI、反馈和 main 的事实来源，本地记录不覆盖远端新事实。
- 复用原独立审阅、submit 报告、准确 PR 头 CI、组合门禁和 promote。
  PR 变头拒绝旧证据；main 漂移保护旧候选，下一步由协调者重新组合和验证。
- 反馈保存后返回，不调用作者 tick、claim 或模型运输。独立项可继续，依赖项等待。
  既有 automatic 作者任务及 native 运输不迁移，不删除作者资源或定时器。
- main 自动发布和发布故障反馈代码未改；合并不等待发布或额外收尾。
  收尾 pending 不把已合并状态改回准备中。只读 status 不推进任务。
- 保存命令及用户请求计时、调用间隔、复用和重新核对项。
  首次就绪来自 GitHub 时间线，作者修复不重置；未知时间保持 null。

## 隔离验证

冻结检查器 b66656a，同一台 macOS arm64，作者源码使用独立工作树。
统一入口为原 scripts/validate --scope pr --stage submit，未更改选择规则。

| 最终源码的步骤 | 实测 |
| --- | ---: |
| project-check | 38.730 秒 |
| 全目标 Clippy | 0.547 秒 |
| 源码包预检 | 0.766 秒 |
| 完整选定回归，1079 项全部通过 | 355.504 秒 |
| 入口总耗时 | 396.049 秒 |

库测试 357 项、native_authority 39 项均继续使用两线程，未覆盖或重写这项改动。
新增用例已绑定正常清单，正常入口实际执行；没有用单独 unittest 代替完整门禁。
定向交接回归 70 项在 11.331 秒通过，最终完整检查还覆盖全部这些用例。

三个独立 CLI 进程的实测为 0.080845、0.076720、0.082783 秒，
依次返回 waiting-ci、merged、merged。第二个进程在第一个退出后启动，
读取同一磁盘记录和 run 42；第三次不重复提升。副作用记录只有一次 push、一次 promote。
外部 GitHub、Git 写入和提升均为 fixture 模拟，不能拿这些数字当真实 PR 合并耗时。

覆盖：反馈后返回、加载原记录、重复调用、PR 变头、main 漂移、缺证据、
查询失败、中断、关闭及重新打开、独立批量项、收尾 pending、发布不随会话退出而取消。
发布失败、渠道保护等沿用既有测试；本轮没有真实发布或真实故障注入。

## 提前发现并修正的问题

子命令初始化失败、还没创建 integration.json 时，原来只有控制台错误。
8fa5e2f 的隔离复现确认未保存日志，现已追加保存子命令输出并给出恢复路径。
首次失败按 run/attempt 不覆盖保存，每次调用保留对应的工作流身份。

另一个回归在修复前确认：handoff 已 merged 或 needs-author，顶层仍可能为 checking。
现在结束调用时同步状态，修复前后测试日志均保留，最终完整门禁验证当前源码。
runner 未启动、准备步骤失败不误派作者修复；验证步骤失败交作者分析，原因未确认不冒认根因。

首次完整检查共 1078 项，其中 1077 项通过，一项未改动 Rust 用例
simultaneous_plugin_entries_elect_one_business_owner 在关闭插件时超过原五秒限制。
首次定向命令遗漏原 TMPDIR=/private/tmp 约定，另外两项产生路径断言差异，
其原始日志也保留；按正常环境整组 8 项通过。后续 0d92264 的 1079 项及
最终 28c31f0 的 1079 项均通过，没有修改 Rust 业务、断言、阈值或并发设置。
这些通过不证明首次退出超时根因已修复，保留为待调查问题。

## 用户用法及报告

首次说“按现有流程合并 PR #N，记录实际耗时”。
遇反馈由用户安排作者或审阅者，再说“继续合并 PR #N”。
合并会话使用原冻结检查器的 pr-queue resume --repository OWNER/REPO --pr N，
清单、新头证据及参数由会话准备，用户不填内部 JSON 或路径。
合并后复用 status --refresh 核对 main 与 PR 实际终态；若状态未同步或又变头，
保留已入 main 的事实，交接当前差异，不能以旧证据宣称新头已合并。
用户主动问发布结果时使用 status --release 或原 release main status，不持续巡检。

隔离报告样例：

> 本轮结果：待 CI，当前 CLI 已返回。
>
> 当前源码：fixture candidate，run 42 / attempt 1。
>
> 已完成检查：仅状态机恢复演练，外部门禁为模拟，不代表真实 CI 通过。
>
> 问题及反馈：等待模拟 CI，没有实际 PR 评论。
>
> 下一步交给谁：外部检查完成后，由用户主动要求继续。
>
> 继续方式：继续合并 PR #N。
>
> 实际耗时：该进程 0.080845 秒；不能当作真实 PR 总耗时。

## 尚未验证及资源

真实 PR 首次就绪至 main、实际用户交接等待、真实 GitHub 故障恢复及跨平台 CI 均未测。
没有可比基线，不报估算提速比例。改进的是退出、交接和复用能力，CI 本身没有被缩短。
索引属于原执行主机的 Git 公共目录；跨主机自动寻址及记录搬迁不在本轮实现范围。
多条活动记录不猜选，主线漂移不自动修改作者源码，恢复仍须遵守冻结检查器和平台约束。
独立实现分支、工作树和全部失败证据保留；临时 fixture 由原测试清理。
其他作者工作树、个人主工作树、会话、配置及日用插件未操作。

源码外证据目录：
/Users/hongyong/.codex/visualizations/2026/11/17/merge-manual-rounds-20261117
有效报告 pr-submit-terminal/VALIDATION.json、TESTS.json；
MEASUREMENTS.json、cli-rounds-validated/、missing-child-log-before/after.json、
terminal-state-before/after.log、首次失败 pr-submit/ 及定向复验日志均保留。
