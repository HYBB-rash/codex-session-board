# 本机原生位置读取优化

2026-11-07，基于远程接入提交 3279763、版本 0.1.59 实现，已部署为 0.1.64。

## 读取与写入

本机状态修改、移动结果确认、标题资格、归档资格及后台位置扫描，
改为从本机 app-server 读取原生分区，不再调用客户端 recent 综合列表。
单目标通过 threadSection/list 校验目录，再用 thread/read 读取 Thread.section；
后台按页读取本机 thread/list，扫描不完整则保留已有位置。
映射文件使用缓冲读取，避免对约 1 MB 文件逐字节执行系统读取。

逻辑分区与后端分区按唯一账号、hostSectionIds.local 对应，保留原逻辑版本。
缺项、重复映射、改名、错误会话 ID 和缺少 section 字段均不能确认操作。
只有明确 section:null 才表示 unsectioned，批量缺项不猜测默认分区。
未管理分区保持 unknown；没有逻辑映射的分区使用后端 ID 作为位置版本，
不按显示名称把自定义分区猜成 Tasks 或 Pinned。旧页面位置版本不匹配须刷新。

专用 NativeReader 仅允许 initialize、threadSection/list、thread/list、thread/read。
原历史读取角色权限不变。移动仍走原 sidebar 写入口，保留同会话锁、
写前版本检查、写后独立读回、丢回执只读确认以及最新输入保护。
远程人工状态和备注仍直接存本地；不接入本机原生读取或启用远程写功能。

## 实测与验证

真实客户端只读测得 119 条本机会话，批量目录及位置读取约 99.9 ms。
发布构建对 30 个目标读取，包含目录验证及映射解析，p50 8.81 ms、p95 10.88 ms；
复用已初始化连接，不代表完整按钮操作耗时。证据 native-direct-buffered.json。
此前综合列表约 5.7 至 5.9 秒是在另一时刻测得，不能作为严格配对基准。

隔离实际二进制 HTTP 验证覆盖远程在线、断线、项目目录扫描挂起和历史扫描挂起。
远程预算设为 6000 ms，每组保存、归档及编辑请求合计须小于 1000 ms；
快照也设独立的 1000 ms 上限，各项实际耗时保存在 remote-isolation.json。
两个挂起场景中，前台操作在释放远程请求前完成，未调用 recent 综合列表。
同 UUID 的本机与两台远程主机互不覆盖，远程断线编辑经重启、恢复扫描仍保留。
这些网络故障由受控端点注入，没有断开用户真实连接或改动日常会话。

证据位于 outputs/native-read-performance-20261107/，含 remote-isolation.json、
真实客户端只读结果、测试日志和完整 acceptance.json。复跑隔离验证可执行
`cargo test --locked --offline --test native_authority remote_online_disconnected_and_scanning_do_not_delay_local_http_mutations`。
冻结的 31 条需求、参考模型与锁未变；本次性能证据不替代既有全部外部验收。

## 日常部署

日常程序、marketplace、安装缓存及运行进程构建一致，客户端 MCP 读回 0.1.64。
沿用端口 18767、原配置与数据目录；124 条本地记录的备注和人工选择字段未变，
两份 MCP 环境注入保持原样；实际显示 119 条本地及 41 条远程会话。
部署后只读测得单目标 p95 10.70 ms、批量 96.25 ms；快照请求约 5 至 6 ms。
健康提示中的一个未读回位置属于原生已归档历史记录，不在当前看板内，
仍按批量缺项规则保留旧缓存，未对它执行归档或恢复。
备份、构建身份、数据比较和测速证据位于 outputs/native-read-deployment-20261107/。
