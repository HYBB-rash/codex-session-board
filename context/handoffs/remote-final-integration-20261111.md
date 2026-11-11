# 最终整合续接

owner: codex
task: context/tasks/active/remote-final-integration-20261111.md
date: 2026-11-11
status: validating_target_environment92

用户授权仅操作专用验收会话，完整验收通过后直接部署，不再请求部署许可。
取消的新建项目任务不重启；只允许在既有M1项目创建一张专用空会话。
Hermes保持关闭，保留生产数据、稳定dataDir及现有归档偏好。

## 最终候选与检查

部署候选固定为92、commit f9c68da06de85bc2f7e739f5df9e638d8f480d3e。
输出根N是本树outputs/final-integration-20261111；真实构建在
N/frozen92/codex-session-board，身份详见N/candidate92-record.json。
FP为701d6a0e05fee12b686fb454d1a9605fac3904205ad578abfcc3e8bc1d0e1eda，
binary SHA为c521f30dfddd0c8caa477a54363cc975f13103323a35ddeb1686fa1a0318170c。
后续纯记录提交不替换这个构建，不可把92的证据签给其他版本。

92正常提交钩子、check/context通过，src业务文件相对91未改。
共用端口租约、子进程立即持有及无poison析构修复3套夹具；64项默认并行通过。
timer仅增加测试discovery同步，保留8秒、lastRun及全部原断言。
91完整528过1个timer失败、92首次MCP Null及另一case端口/析构失败均原样保留；
原MCP Null缺完整回执，不能追认与端口失败同因。
92首次完整525过4失败是共享target复用了已删除precommit目录的测试ELF；
原失败保留，不复制旧临时目录来通过。改用新的target92-acceptance-02后，
完整529过0失败，automated_passed=true、release_ready=false、exit2。
见N/release92-isolated02.json及full92-isolated02日志、settings、exit。
后续冻结构建验收继续使用独立target，避免CARGO_MANIFEST_DIR缓存混淆。
冻结31条需求、参考模型和锁未改；完整31/16及内容审阅仍未签收。

23项真实浏览器组件在N/browser92-01通过；只接测试局部假刷新，不签native/PWA。
P私有官方CLI运行在N/private-protocol92-01：19过0失败2pending，
5私有chat、6start、9个loopback fake HTTP、2回调、1interrupt，清理完成。
配置及实际路由支持0真实provider，不宣称内核网络隔离。
N/private-protocol92-review已独立审阅；native UI及空会话持久两项仍pending。
P不是N/R真实输入额度；本候选尚无新的canonical模型输入，旧真实账本不重置。

## 本机L与独立入口

S的LOCAL-TARGETS-RELEASED.json明确在09:21释放本机A/B，不能再写成仍占用。
独立基线见N/local-baseline92-01：A=01a1bad2-498d-73b1-bd74-eb248f61985e，
B=01a1bad2-4c06-7162-900d-8d067d510dc2；原归档为true、原生分区为待归档。
两卡现已由本轮取消归档。仅L启用本机写入；启N前先结束L并独立恢复核对。
L configId=32d96d53e1d922fa，port19283；
config在N/scoped-runtime92-prep/bindings-01/L/config.json；
data在N/scoped-runtime92-01/L/data；shortroot=/run/user/1000/sbf-01a1b4de-l92-01。
guard PID3364707、board PID3365675；身份在L目录的guard-process/board-process.json。
统一exec包装会话分别17985/43900；停止前仍须复核PID/starttime/exe/config，不能仅信旧PID。
本机模型与创建grants均0；不停止canonical用户daemon或其他任务的guard。

L官方独立插件为codex-session-board-final92-l@session-board-final-01a1b4de-l，
displayName为Final92 测试看板 L，单server为session_board_final_01a1b4de_l。
绑定包N/final-plugin92-L01，CLI安装回执N/final-plugin92-registration01；
前后生产82 PID/build/config及S/prod缓存SHA未变，没有请求全局reload。
模型可见工具实际叫board_view，先前open_board检索0命中的推论已撤回。
正确名称检索也尚未返回新L，但不把有限检索当全catalog不存在的证明。
官方单插件codex URI被CUA浏览器URL安全规则拒绝，未导航或交给桌面处理；
不换浏览器、raw CDP、间接执行或xdg-open绕过。见L/native-entry01-result.json。
Native Chrome ctrl+t在20秒超时并重置CUA内核，效果未知，不推测权限原因。
原生drag明确不支持；需要真实客户端操作的项保持待验。

L的实际MCP初始化、tools/resource/board_view/snapshot已通过，退出未停既有HTTP；
见L/mcp-readonly01。L/manual-protocol01验证过期版本拒绝、空阻塞原因拒绝且无修改，
B手工进行中/中文emoji备注保存、两次刷新保留manual与inputVersion、标题只换前缀。
实际App read_thread确认B已保存[进行中] 🗄️｜验收0.2.83选项澄清流程，
lastTurn仍为01a1c761-0644-7462-a4a0-0f19bf4b7383，没有新模型轮次。
本轮首次标题写入前，A/B正文已从最初同名fixture被外部改成emoji名；
旧基线与pre-own-write-baseline均保留，不把外部改名归因于本轮或清理时覆盖它。
B现有备注为0.2.92专用验收B的中文备注；L结束先恢复私有状态/备注，
停止其写者后再恢复本轮实际改过的canonical前缀、原归档及原生分区。
IAB实际19283页面已显示两卡及上述状态/备注，中文标题搜索过滤1/2后已清空恢复2/2。
IAB不是native MCP或独立PWA；未签实际拖拽、安装或完整UI通过。

## M1与新R

H=remote-ssh-codex-managed:f6cdae6b-9c38-4789-9071-db5e90f93a7c；
远端A=01a1b88f-70fd-73c1-b10b-10f856dfd5a6，B=01a1ba66-08e5-7dd3-a1e9-67e5b3315907。
12:16只做一次官方idempotent daemon start；manager61763/worker61810、
managed0.167.0 image SHA030316ecfbb41f4e7946e584b0063ef333b7a36cd312ba349f2f6869e39b62cc。
control路径是官方symlink；旧strict helper拒绝symlink不能证明daemon退出。
N中第二次canonical只读观察在13:17实际before/initialize/after全过且身份一致；
仅initialize/initialized、0模型0业务写，Peer UID501及codexHome精确匹配。
01是父端提取README代码块数量assert失败，未建transport/发RPC，失败记录保留。
02实际复用v2通路，不称为v4回调实测；后续v4封存81项SHA已独立复核。

14:57只读检查发现managed/current已换为0.167.1，现owner4421/parent61810；
升级来源未定，未由本轮重启daemon。旧R预检因旧image校验拒绝，尚未init。
15:14的canonical-live1671-observation02在新image上实际握手通过，
before/after同实例，Peer/home匹配、reader退出、0模型0create0越界写。
其01遗漏initializer旧SHA校验，停在transport/RPC前；失败原件及分类保留。
R-full01首init前允许A标题/恢复的受限故障准备修订，原23文件已封存；
唯一requestId/grant/config/port不变，首create必须faults为空，不补或重置额度。
继续前须审阅修订封存、复核当前0.167.1身份，不能拿旧版本预检签新连接。

当前global-state确有H及cwd /Users/hongyong/Documents/副业项目调研，
nativeId=413e40a6-c61e-44de-ba5a-85f930d82cba；App list_projects当前仍没返回H。
14:02只读SSH stat确认目录存在、非symlink、owner501/0755、可读且mode可写，
见N/m1-project-directory92-observation01；未写远端文件、未创建项目或会话。
App桥与真实输入页仍需实际验证，不从SSH或注册记录推断它们通过。
新的R准备在N/scoped-runtime92-R-prep；旧bindings-01/R仍blocked，未修改或初始化。
R-02零create方案及R-full01完整single-create准备均是独立材料；
首次init前冻结准确grants/唯一requestId，不能给已初始化的零create Store补额度。
R Root是native5，remote child为native_sections=false且legacy_mode=false；
local owned为空，不写本机A/B。远端B保持对照，外部写仅A及唯一createdOwned。
R2目标及新空UUID须真实创建/独立读回后另冻结v4新批次；不虚构UUID或启用旧账本。
继续前读取新R目录实际execution/清理记录，只有静态配置不算启动或验收。

## 接续与部署门禁

先完成真实入口/系统PWA与N/R controller及UI绑定，再启受限真实输入批次；
N与L本机写集串行，R只用H专用目标。剩余清单见N/remaining-acceptance-boundaries。
不得把旧83、S88、私有P或IAB网页结果代签新候选native/创建/active-owner。
完整31条及16项必须同最终构建/对应配置、exit0、release_ready=true且内容经独立审阅。
通过后按既有授权直接部署；生产当前82、port18767、PID1828966，仍须新鲜复核。
生产旧六绑定须用旧82 binary及旧config先停止；92不能解析旧completed绑定来stop。
备份一致DB/WAL及配置、保留回滚，迁移五绑定并保持dataDir、本机1800/远端关闭。
不要把只读迁移演练中指向生产dataDir的新配置当测试启动。旧83树及全部失败证据保留。
