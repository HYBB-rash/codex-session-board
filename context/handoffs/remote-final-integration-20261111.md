# 最终整合续接

owner: codex
task: context/tasks/active/remote-final-integration-20261111.md
date: 2026-11-11
status: validating_fixture_reliability92

用户授权仅操作专用验收会话，完整验收后直接部署，不再请求部署许可。
本树从2f63a5a分区实现建立，分支codex/remote-final-integration；
88已合并Main be7d079记录，89又合并S5571ff4的清理/生成预算修正及恢复基础。
90为6e712a0，补全持续告警/observer/PWA/host.error；完整钩子、check/context、
11恢复回归与20布局场景通过。91为8225fd7，整合门禁后完整钩子/check/context通过。
旧树remote-restore-acceptance在f8dad19记录85，其check和context已通过。
旧83身份、部分真实实测、失败和独立review保存在旧树outputs/remote-restore-20261110。
原cache83路径已消失，当前同名槽为另一任务88；binary83-retained.json记录了同SHA原构建包。
不恢复或覆盖该共享槽，最终入口用本会话独立命名空间。

门禁Gate89为69fc35e，40项Python、478项Rust、check/context通过；
保留S原18和C九项原断言，新增3项完整CLI回归；静态审阅原方法AST一致。
已有full CLI探针复现5次私有附件覆盖及2次缺SHA被接受；假夹具通过不签业务。
静态原集合并集526为下限，新增回归相应绑定。保持冻结31条及锁，不降低断言。
91实际绑定529；首次完整gate为528pass/1fail、16需求pass/14pending/1fail，
唯一失败为built_service_archives_on_its_timer_and_publishes_the_result_without_an_archive_request。
原失败、单项重跑1过、32项组重跑全过都保留，不能用重跑清除失败。
独立18次私有诊断中3档慢归档复现实际archived=1、备注保留、结果被后续空run覆盖；
原Full91缺失败快照，不追认其确切原因。92采用仅测试discovery屏障，
保留8秒/lastRun/全部原断言；原型正常及六档慢归档全过、Err路径释放已核验。
92首次提交钩子另在MCP titlePending断言得到Null，原日志缺完整replies，未定因。
补诊断后的前10组board_io全过，第11组却在另一个HTTP case遇到端口占用，
随后Drop锁poison二次panic使进程abort；不当作原MCP失败同因重现。
已识别本树debug exe/当前UID/私有config的残留审查为0，不触及生产或其他树。
共用tests/support/fixture.rs租约：按实际自动端口范围排除、同UID跨进程文件锁，
Arc持锁直到相应子进程清理；guard即时拥有spawn结果，失败也kill/wait，
Fixture收尾无poison unwrap。三个fixture均复用，不修改生产bind或调度。
board_io 28、native_authority 32、column_sort 4默认并行共64项通过；
原MCP完整replies诊断保留，完整钩子及新候选冻结尚待执行。
不协作进程显式bind的最后窗口仍存在；未引入生产监听FD继承接口。
完整串行适配器仅外层预算改1800，内部600不变；上一轮串行Rust测试累计约912秒。
创建并行工作树前再次确认协调树干净、记录base；写任务各自worktree及branch。
工作者仅改其分配文件及唯一结果记录，不改协调树或任何真实会话。

本机A/B仍由state-section-release-20261111/local-run-86-01占用，未释放前不写。
远端A/B已由旧83测试恢复原名并归档；没有新增真实远端模型轮次。
刚重测M1只读SSH成功，但桌面read_thread仍host unavailable；原生菜单点击35秒超时，
未确认设置打开，不推测输入权限原因；已请求用户在设置连接里重连。
Linux CUA不支持实际drag，不能合成事件签原生UI。取消的新建项目任务不重启。
原生坐标点击和Alt+f均超时，未确认连接设置打开，不推测OS权限原因。
11:04再次exact read_thread/H仍Unavailable；已找到新日志但无当前连接错误，
根因未确定，旧client-not-found不能当作当前失败解释。SSH和桌面路径分别核验。
91独立浏览器23组件通过，已复制E/browser91-copy；不签系统安装和原生UI。
91只读迁移演练已确认生产旧六绑定可生成五绑定、catalog预检通过，
数据目录/本机1800秒/远端关闭均保持，源配置不变，无DB或会话操作。
P19复用入口已准备但未启动，私有和真实创建/start均0；暂缓来自协调者，
并非用户暂停，原部署授权持续。下一候选使用新身份和独立短socket，旧账本不重置。
最终冻结后新取完整31条及16项、同FP/binary/config、实际UI和内容review。
插件注册优先官方CLI和本会话独立namespace；没有已证明的单插件热加载，
不全局reload、不重启桌面；A/B未释放前不暴露能写它们的配置。
release_ready=true并退出0后部署，保留稳定dataDir、用户归档偏好及关闭的Hermes。
