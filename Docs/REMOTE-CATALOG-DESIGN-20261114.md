# 切换主机后显示目标项目与会话

日期：2026-11-14。状态：设计与接口取证；业务功能尚未实现。

## 决定与交付范围

在会话看板增加目标主机 Codex 本地项目与会话目录读取层。用户只需登记并连接主机，不必在操作机逐个重建目标项目。
切换主机后显示该主机全部当前项目，包括零会话、多目录和操作机不能访问的项目；会话归属按目标数据计算。
活动历史与归档分别展示，提供“活动／归档／全部”筛选。默认活动，搜索、项目筛选及批量操作继续固定当前主机。

“完整”指目标后台普通持久历史，加目标桌面已发布的置顶和自定义分组引用。
原生窗口中尚未发布、仅留在内存的当前空草稿不属于这个承诺；现有公共后台接口不能保证镜像该窗口的全部运行时状态。
底层 attached、ephemeral、执行记录不会因为存在于 SQLite 就自动成为独立普通会话。
本看板自己确知的未完成新建请求继续单独展示，不把所有旧缓存当新草稿。

首版只扩展读取与展示。新发现的目标原生项目不接入旧客户端远程项目动作，能力明确为不可新建、不可移动、不可用旧入口打开项目；项目筛选与打开已有会话分别处理。
已有 host+thread 的状态、备注、标题、归档及打开路径保留；“有目录读取能力”和“已验收原生 GUI 打开/写入”分开报告。

## 已实际验证

证据目录：
`/Users/hongyong/.codex/visualizations/2026/11/14/01a1da30-d415-7413-aa72-bf742f3110af/remote-catalog-20261114/`

| 核对项 | Mac | Fedora |
| --- | --- | --- |
| 实际 initialize 后台 | 0.169.0，uid 501，/Users/hongyong/.codex | 0.169.0，uid 1000，/home/herman/.codex |
| project/list 全部分页 | 9 项，3 页，逐项 read 一致 | 7 项，4 页，read 一项成功 |
| 当前 legacy 项目／历史映射条数 | 9／11 | 7／9 |
| global-state 远程读取 | 约 1.25 MB，可完整 base64 解码并解析 | 约 1.83 MB，可完整解码并解析，首次 read 约 68 ms |
| 普通持久历史，全部 provider | 活动 21，归档 897 | 活动 16，归档 154 |
| owner 选择 | 单一迁移 namespace 与 marker 精确匹配；account/read 身份 hash 也匹配 | 两项精确匹配均成立 |
| threadSection/list 与成员分页 | 6 个 backend section，双来源查询并集成功 | 6 个 backend section，双来源查询并集成功 |
| 当前账户 custom 引用 | 1 个明确 local、13 个明确 foreign、2 个待定 host | 15 个明确 local、1 个明确 foreign |

Mac 本地取证验证 control socket 的 peer PID、可执行文件、uid、inode，前后保持同一后台。
Fedora 另跑了仓库原样登录 wrapper、身份前导帧与 remote_turn_proxy，而非仅测试绕过它们的裸接口；7 项和大 JSON 均通过。
另从 Fedora 发起真正的 SSH 连接返回 Mac，经同一 wrapper/proxy 读到 Mac 的 9 项和完整状态，初始化明确是远端 0.169。
反向取证用临时本地 agent 转发签名，私钥不落远端；主机公钥严格核对，没有修改 SSH 配置或信任文件。取证进程和 agent 已退出。
反向探针早期 host-key FD 方案失败已保留；最终用本机已核公钥的 KnownHostsCommand 成功，不关闭严格主机校验。

Mac 日用 GUI 自带 CLI 是 0.167.0-alpha.4.1，与上述共享后台不同。
旧 CLI 已实际导出 experimental schema，包含 project/list、project/read、fs/readFile、fs/getMetadata；这不等于日用 GUI 的 stdio 后台已跑过同一套 RPC。
原生 SSH 代码及实际反向取证均证明：目标后台由远端 PATH/CODEX_HOME/control endpoint 决定，不由操作机 GUI 自带版本决定。

## 查明后必须修正的假设

1. `sourceKinds:["cli","vscode","appServer"]` 不等于原生 interactive。0.169 默认还包含 custom atlas/chatgpt，反而不含 exec 和 Mcp/appServer。首版使用 `sourceKinds:[]`。
2. `modelProviders:null` 会按配置缩小范围；Mac 归档少 19 条。目标为整台主机持久历史，因此固定 `modelProviders:[]`、`currentAccountOnly:false`。
3. DB-only list 的 threadSource 全部为 null，不能当作 user，也不能据此删除内容。原生普通列表没有 subagent 标签白名单；Mac 实际存在 56 条 interactive 且持久标签为 subagent 的记录，必须保留。
4. 普通 list 排除空 preview 和 attached，并非“数据库总行数”接口。Fedora 差集 13 条精确分为 6 条非 attached 空 preview、7 条 attached，其中仅 1 条空 preview。Mac 另一差集 9 条全部空 preview。
5. `includeTurns:false` 返回 turns=[] 是请求语义，不证明实际会话没有历史消息。不能拿该数组签“空记录”。
6. 新 main 的 includedThreadIds 在安装的 0.169 尚不存在；传入不报错却返回无关行。支持判断必须核对安装版协议和返回语义，不能只看 RPC 成功。
7. 历史 ID mapping 不是当前项目目录。两机均多出 2 条旧 mapping；只枚举 mapping 会复活已删项目。
8. 旧 snapshot 代码把 sectionVersion=null 的缺席行保留。原样业务方法探针已经复现，而远端恰好强制 native_sections=false，必须新增独立 presence。

## 连接和接口契约

运行时统一经目标 `App::connection`，复用 Binding、endpoint/uid/HOME/CODEX_HOME、登录前导帧和 initialize 来源校验。
新增 `CatalogReader`，只允许 initialize、project/list、project/read、thread/list、metadata-only thread/read、threadSection/list、account/read、fs/getMetadata、fs/readFile。
把新 role 纳入现有 remote_turn_proxy 分支；保留已验证的 framing 和共享连接策略，不另建业务 SSH 后台。
fs 路径由已核实的 codexHome 生成，只准读取该目录的 .codex-global-state.json；不接受前端任意文件路径。
thread/read 强制 includeTurns=false；account/read 强制 refreshToken=false，只用于账户身份选择，不持久化 email 或完整账户响应。
目录读取没有 project/import/update/create、thread/metadata/update、start/resume 或 turn 执行权限。

每次新 source generation 探测能力。0.169 是本轮实测范围；旧版方法存在于 schema 只作辅助证据。
必需接口明确 unsupported 时保留旧能力和缓存，显示不支持完整目标目录；首版不开发未经验证的 SSH/SQLite fallback。
网络失败、权限失败、坏 JSON、来源错配不降级成“空目录”。Fedora 已分别实测不存在文件 os error 2 与权限 os error 13，两者 RPC code 都是 -32603，不能只按 code 分类。

普通历史请求：

```json
{
  "limit": 100,
  "cursor": null,
  "sortKey": "created_at",
  "sortDirection": "asc",
  "sourceKinds": [],
  "modelProviders": [],
  "currentAccountOnly": false,
  "archived": false,
  "useStateDbOnly": true
}
```

首轮完整扫描用稳定 created_at 顺序；日常新增发现保留现有快速扫描，周期完整核对负责 presence。
归档改 archived=true 独立分页。projectId、sectionId、cwd 均省略，不把显式 null 误当“全部”；0.169 的 projectId:null 表示未分配。
不使用 includedThreadIds 或 includeAttachedThreads 作为全量开关。后者还要求非空搜索词。
仅对置顶缺项、真实引用或归属冲突做单条 metadata read，不为 918 条记录强制额外读一遍。
Mac 确实实测过 918 次只读 metadata read，865 ms、约 1.77 MB、0 错误、数据库 0 行变化；这是本地实验，不是远程性能承诺。

## 最小数据与身份

SourceScope 保存 hostId、confirmedPeerFingerprint、codexHome、generation、capabilities、observedAt。
ProjectRecord 保存 key、hostId、origin、serverId?、legacyId?、name、全部 roots、position、来源 presence、capabilities。
origin 区分 targetServer、targetLegacy、clientRemote。主机隔离后再区分 origin/id，目录相同也不合并身份。
ThreadObservation 保留原有 thread UUID 与 host；backendProject 为 Missing／Unassigned／Assigned 三态，不再在 discover 时丢掉。
LegacyProjection 仅保存当前项目、有效映射、明确 assignment/projectless、迁移 checkpoint、pending deletion、必要工作区与持久引用。
Membership 保存 assigned／explicitlyProjectless／unassigned／unresolved、projectKey、basis、pendingMigration、issue。
Coverage 分开保存 projects、ordinaryHistory、archive、nativeSections、legacyReferences、membership 的 complete／partial／error／unsupported，以及查询范围和时间。
人工状态、备注、同步任务、归档 tombstone 与在线 presence 分离，不因一次扫描缺席而删除。

## 目标状态投影和目录合并

1. 通过目标 fs 接口读取状态，解码后只保留白名单字段，释放完整 JSON；不保存凭据或会话正文。
2. 使用同一文件的 electron-renderer-content-owner，精确选择 electron-owner-value:<key>:<owner>；有 owner 时该值缺失不回退旧顶层同名值。
3. atoms 与原生一致：shallowMerge(electron-owner-atoms:<owner>, electron-persisted-atom-state)，live 同名 atom 整体覆盖，不深合并账户桶。无 marker 的旧 schema 才只用 live atoms；不合并其他 owner。
4. 目标本地 migration identity 是 local:<目标 codexHome>，不是操作机为该远端分配的 hostId。
5. project/list 跑完后的当前集合为 backend 目录。全部 roots、零会话项目都保留，不调用操作机 Git 检查来删除远端项目。
6. 用当前 legacy 项目及有效 mapping 建关联；metadata 没 localProjectId 仍可工作。只有 mapping 而无当前项目，不创建目录行。
7. 当前 legacy-only 项仅在已知 schema 的迁移状态未完成、无历史 server 映射/metadata 关联且无 pending deletion 时显示 targetLegacy。映射指向已消失后台项目、已确认迁移却缺关联、迁移状态不可解的，记录冲突与 partial，不复活幽灵目录。
8. 匹配当前 namespace 的 pending deletion 显示删除中、停止新动作，不替原生执行删除或迁移。
9. 操作机 clientRemote 项保留旧写入上下文；主机目标目录不受它是否登记限制，也不按同路径把两个 origin 硬合成一个。
10. owner 表示目标桌面当前账号配置，不证明与操作机登录同账号；后台“整机历史”与 owner overlay 的范围分别记录。

账户 selector 已实测：account/read 响应顶层 userId 与 workspaceRouting.chatgptAccountId 做 JSON 二元数组的 SHA256，必须等于 marker。
两端均精确相等；只用已验证 accountId 选择 sidebar-custom-sections-v3 的账户桶，缺失或不匹配标 accountUnresolved/identityMismatch，不任选第一桶。
前后读取的账户指纹也纳入 legacy generation 校验。抽出统一纯 owner 投影，不能直接复用当前只返回 ownerAtoms 的读取而漏掉 live 覆盖。
marker 类型错误、owner namespace 冲突或私有 schema 字段类型变化使 legacy partial；保留公共后台结果，不挑第一项或把异常当成功空值。

## 归属状态机

先解有效当前 ID，处理显式成员，再考虑路径。不得以 cwd/Git 覆盖手工归属，也不得把后端 null 自动等同用户主动移出项目。

| 情况 | 处理 |
| --- | --- |
| 后端字段 Missing | 同 host/home/peer/version 且两轮账户指纹均有效并相同，才保留仍有效的已知归属；标记 previousObservation、lastKnownAt 与 backendProjectMissing，membership coverage 为 partial。显式 null、新来源/账户、未知账户及项目删除不续接；当前明确兼容意图优先于历史观测 |
| thread 在 pending 集合 | 未应用的有效 legacy assignment 或明确 projectless 优先 |
| 两个迁移完成标记均未成立 | 有明确 legacy 意图时保留；不能让尚未迁移的 backend null 冲掉它 |
| 已完成对应读取/迁移且 thread 不 pending | 以有效 backend project 为准；与旧意图冲突先 metadata read 并复核同一状态 generation |
| backend null、旧 assignment 存在且已确认迁移 | 按已清除处理，阻止 cwd 又将它分回去 |
| explicit projectless | 不做工作区、cwd 或 Git 推断 |
| 没有明确归属或清除证据 | 可使用目标工作区提示、目标路径最长根；同目录多项目平局为 unresolved |
| ID 已删、映射冲突、非 local assignment、host 不符 | 保留 issue，不伪造默认工作区或跨主机归属 |

原生 per-thread 迁移输入还包括 threadAssignmentsMigrated、threadAssignmentsReadMigrated、pendingThreadAssignmentIds。
实现为纯函数并固定组合矩阵：global false/read true/不 pending 不等于该 thread 尚未处理；global true 但该 thread pending 也不能丢掉旧明确意图。
legacyPending = pending.contains(threadId) || (threadAssignmentsMigrated!=true && threadAssignmentsReadMigrated!=true)；仅有明确有效旧意图时应用该优先级。
两端实际 projectsMigrated=true、threadAssignmentsMigrated=false，pending 分别为 Mac 260/Fedora 111，read 标记均缺失；Mac 83 个明确 local assignment 均存在于普通历史范围，不能被后台 null 冲掉。
目标窗口尚未发布的 pendingCreated 集合不可跨机重建，归入运行时草稿边界；本看板只覆盖自己有收据的新建。
远端路径不在操作机 canonicalize。首版不新增任意 Git 命令；不能以已实测的普通目录场景冒称任意 worktree 拓扑都已确认。
有明确项目归属的 worktree 照常显示；无明确归属且无可靠工作区提示时列“未确定项目”，不编造准确归属。

## 持久引用补读

普通分页不是所有原生运行时 recents 的镜像。只补已发布的目标置顶、自定义 section 引用和本看板可信 draft 收据，不触发原生 migrateLegacyPinnedThreads。

- threadSection/list 用 cursor/limit 全分页；当前实测内建 pinned ID 为 01984de2-8f74-7c91-a3b2-5c5e937cf318，无 kind/isPinned，不按翻译名称识别。
- 每个有效 backend section 以具体 sectionId、created_at asc、archived=false、modelProviders=[]、useStateDbOnly=true 分页。sourceKinds=[] 与该版本全部受支持枚举分别查询后按 UUID 去重，覆盖 custom atlas/chatgpt 及明确发布的非普通来源；不扩大普通历史默认范围。
- pinned-thread-ids 是共享旧引用；owner-scoped app-server-migrated-pinned-thread-ids-by-host[local:<codexHome>] 只是已迁移集合，不能把其中取消置顶的旧值复活。
- 有效 atoms 的 sidebar-custom-sections-v3[verifiedAccountId] 取 sections、threadHostIds 及迁移/删除标记；只解析 codex:thread:local:<规范UUID>，itemKey 的 local 不证明主机归属。
- threadHostIds[threadId]=="local" 才是明确本目标；其他 host 排除。缺失作为待定候选，只能在已绑定目标后台定向核实，不能自动连接目标桌面的其他远端来找会话。
- hostSectionIds["local"] 是本目标 replica；appServerDeletedSectionIds 是 backend ID，appServerLegacySectionIds 是逻辑 ID，不能混用。pendingDeletionHostIdsByItemKey 含 local 则不补回；appServerMigratedHostIds 含 local 后以后端成员为准。

仅对未迁移且不在普通/section 结果中的真实 pin/custom 候选 thread/read(includeTurns=false)，不用 includedThreadIds。
补读需核对同 UUID、非 ephemeral、非 attached；archived 不能由 status/notLoaded 推断。
归档校验复用已完整的对应来源归档 ID 集；普通范围外的引用才补 default+全部受支持来源的归档并集，不将该校验范围直接灌入普通归档 UI。
缺附属/归档证明或找不到的引用标 unresolved；项目 assignment 本身不证明可见性，也不删除记录来掩盖冲突。
backend section 成员可直接补入，无需逐条 read；不能因持久 threadSource=subagent 删除 interactive 会话。
首版只补成员，不重建整个原生 section 排序/折叠布局；不会启用 remote.config.native_sections 或原生分区写同步器。
0.169 枚举固定为 cli、vscode、appServer、exec、subAgent、subAgentReview、subAgentCompact、subAgentThreadSpawn、subAgentOther、unknown。
未知协议版本没有“全部来源”证据时只保持已证实范围，不因接受旧枚举而宣称穷尽。Mac 两个缺 host 的旧引用已实测不在 Mac DB，却均在已连接 Fedora 的列表中；不默认 local 的规则有真实反例。

## 快照、缓存和刷新

连接或显式刷新后立即扫描；切主机仍只改变显示范围，不开启自动连接。离线先显示按主机隔离的上次成功缓存及时间。
先读状态 A，再分页目录/历史/归档/引用，发布前重新读状态 B；owner/hash/source generation 变化则有界重试，连续变动标 partial。
只在全部页成功、游标不重复、预算未截断、引用处理完成后发布对应 complete generation；失败保留上次完整快照。
分页预算仍有限。达到预算保留参数指纹及 cursor，显示“尚未读完”并继续，不每轮从头截在相同位置。
generation 必须带 host/source/查询范围；快切主机、换 endpoint 或晚到响应不能覆盖另一个来源。
在线主列表只计本轮已核实 present；缓存未知单列历史缓存。完整普通列表缺席不证明数据库删除，也不删除人工信息。
归档记录独立范围，默认关闭其观察/自动动作；非普通来源的 section 补充默认只浏览，不扩大自动观察/自动归档范围。
手动归档与标题动作继续遵守原有确认及来源门槛，不因新增视图扩大自动动作。

首版不依赖 fs/watch：实际只测了订阅/取消，未验证替换文件通知。选择已测过的 metadata/read/hash。
默认每 30 秒 metadata 核对，有变化才重读；每 5 分钟强制内容核对，连接、切入已连接主机和手动刷新立即核对，连续请求 2 秒合并。
目录/历史全量核对每 5 分钟并按主机错峰；已有线程快速发现不替代完整核对。显示最近成功时间，不承诺跨 JSON/DB 原子快照。
这些是选定的首版策略；实现后测端到端耗时及前台响应，不能把 68 ms 文件 RPC 当整体加载 SLA。
状态 decoded 上限 16 MiB，保持现有 64 MiB 转发帧限制；超限给可见错误和 partial，不截断为成功。只持久化字段投影。

## 动作矩阵

| 动作 | 首版决定与验证边界 |
| --- | --- |
| 目录、历史、归档浏览 | 使用新只读目录，完全不要求操作机登记项目 |
| 打开已有会话 | 原生解析/导航/renderer 原样内存 fixture 已通过，不读项目登记；真实 GUI 选中与远端加载另行签收 |
| 状态、备注、标题、归档 | 保持原 host+thread 路由和版本/source 检查；本轮未执行真实业务写操作 |
| 旧 clientRemote 新建 | 沿用既有可恢复请求，不换 ID 语义 |
| 新 targetServer/targetLegacy 的项目动作 | canCreate=false、canMove=false、canOpenProject=false；项目筛选仍可用，不借旧 nativeId writer 或项目 URL 冒充已支持 |
| 离线、未知存在性、来源变化 | 允许缓存浏览，遵守原有写入门槛；不因显示项目而获得写权限 |

准确打开契约为 `codex://threads/<UUID>?hostId=<操作机已登记远端hostId>`；host 不是 hostId 别名，也不能传目标内部 local。
当前看板 conversation_url 已正确编码 hostId。实际原生解析器、导航 callback、renderer 路由原样内存探针已执行通过；项目管理、globalState、catalog、Git 与后台依赖访问即抛错仍成功，证明没有项目登记前置条件。
真实 GUI 视觉选中、认证与远端加载仍是目标验收项；本轮未切换用户活跃会话，不把内存路由成功冒称真实 GUI 验收。
若后续目标包括“在新发现的目标原生项目里新建/移动”，单独设计并在隔离真实后台验证原生创建、归属和请求恢复，再安排写入验收。
本次不把这些新增写能力偷偷纳入读目录任务。

## 固定实施与检查顺序

1. 连接与 DTO：CatalogReader、受限参数、capability probe、保留 projectId 三态、sourceKinds/provider 契约。
2. 目录与成员：owner 投影、分页、当前 ID 关联、纯归属状态机、持久引用补读；只读 RPC transcript 作门禁。
3. presence 与 UI：独立在线范围、成功 generation 缓存、主机/项目/归档筛选、时间与错误状态、按 origin 限定动作。
4. 隔离回归：双主机同 ID、不登记 remote-project、空目录、多根、迁移组合、明确移出、同路径冲突、坏文件、断页和晚到响应。
5. 目标只读验收：Mac/Fedora 各对照全项目和普通历史及持久引用差集，验证零操作机项目登记、手工跨 cwd、无项目与归档。真实 GUI 打开和既有写动作分开留证。

每一开发 PR 保持单一改动、版本不变，按既有提交快检、完整组合门禁和 31 条验收要求执行。
这份设计及本轮探针不等于实现、部署或完整验收通过；缺少目标环境证据的项目继续待验。

## 预计修改入口

| 入口 | 明确修改 |
| --- | --- |
| src/rpc.rs、src/rpc/forwarding.rs、scripts/remote_turn_proxy.py | CatalogReader 与参数约束、既有代理 role 分支；代理协议本身不替换 |
| src/projects.rs、src/remote/project_registry.rs | 分离目标目录与 clientRemote 写上下文；纯 owner/目录/归属投影 |
| src/observer.rs、src/model.rs、src/store.rs | 保留 backend project 三态、成功 generation、独立 presence 与 coverage |
| src/remote/scans.rs | 普通/归档/section 分页与续读、按来源补充校验；不复用旧三枚举作全范围 |
| src/app/remote_snapshot.rs、现有前端 | 当前在线与缓存分开，主机优先的全目录/项目筛选和动作能力 |
| src/app/drafts.rs、src/native_sections.rs | 隔离原有写语义与共用纯 owner 读取；不把新 serverId 传给旧 nativeId writer |

## 证据与源码锚点

上游安装版锚定 rust-v0.169.0，解析到 commit d32063f96c2dff1dc96af839b3952abb6acfeda4。
已冻结的 main b1573f102413e96a6481087922bf425231222dee 仅用于比较，不能把它的新字段当安装版能力。
官方目录：openai/codex 的 app-server README、app-server-protocol v2/thread.rs、rollout/src/lib.rs、state/src/runtime/threads.rs。
当前客户端已安装 ASAR 的 recents、project migration、owner selector 与 SSH spawn 分别核对；它是本机版本证据，不是公开稳定 API。

| 证据文件（均在上述源码外目录） | 证明范围 |
| --- | --- |
| mac-rpc-evidence.json、mac-followup-evidence.json | 当前后台身份、项目/文件、分页、过滤、差集、metadata 读回成本及零 DB 行变化 |
| mac-migration-evidence.json | 当前目标 migration checkpoint 与 local:<codexHome> 命名空间；不是整份私有状态副本 |
| fedora-rpc-evidence.json | 首轮项目分页、1.83 MB 文件、标准来源分页、错误与 watch 订阅能力 |
| fedora-diagnostic-evidence.json | 原样登录/转发链路、default sources、差集、owner/account hash、section 双查询与权限错误 |
| reverse-ssh-evidence.json | Fedora 发起到 Mac 的真实 SSH、远端 0.169、9 项目录、owner/account hash 与 section 双查询 |
| reference-cross-check-evidence.json | 两个无 host 旧分组引用不在 Mac DB、实际在已连接 Fedora 列表；不是新自动发现连接 |
| deeplink-receiver-results.json、deeplink-receiver-probe.js | 原样原生接收/导航/renderer 路由执行；hostId 成功、host 反例及禁止项目依赖，非真实 GUI 验收 |
| reverse-ssh-attempt1.json、reverse-ssh-attempt2.json | 早期取证失败保留，未当成功 |
| bundled-schema-evidence.json | 日用 GUI 自带旧 CLI 的 schema 符号，仅静态能力 |
| snapshot-results.json、snapshot_probe.rs | 原样 Rust snapshot 分支内存复现，不是完整 App 集成或现场数据重现 |
| native-empty-visibility-results.json、native-empty-visibility-probe.js | 原生 recents 三个方法原样内存探针，不代表空记录必然在目标窗口缓存 |
