# C2 在线与离线缓存构造首件

## 结论及净效果

当前没有找到可独立净删的构造逻辑，建议以调查和独立复核结项。  
`App::new` 只有一份 22 字段初始化，在线和离线入口均调用它。  
其中 5 个字段只供根入口业务使用，但挪到另一类型不能直接删除这些业务状态。  
其余 16 个字段按主机隔离，`mapping_store` 在线共享根库。  
在线构造多做一次随后被替换的 `Arc::clone`，没有重复打开根库。  
没有证据表明新增构造接口或根/主机类型能净删代码，也没有测得性能收益。

本次业务源码净增 0、净删 0，新增类型和构造接口均为 0。  
不修改外部协议、SQLite 格式、缓存格式或冻结六状态模式。  
若以后拆分类型，需先完成调用迁移设计并重新量化，不能把移动的初始化算作删除。

## 固定对象与核对方法

- 工作树及分支：`/Users/hongyong/.codex/worktrees/b3f7/codex-session-board`，`codex/refactor-c2-remote-construction`。
- 2026-11-21 fetch 后基线：`b859880165908a848a4982d2cfd9b722e9aeb912`，与当时 `origin/main` 一致，起始干净。
- C1 只读对象：`f15754129ac64a1dd02ec93abb00f9237bf0c9f8`。
- 用 `git show` 读取 C1 的 `archive_batches/host.rs`、`remote/instances.rs`、`remote/cache.rs` 及归档调用方。
- C1 的两个远程构造文件与本任务基线相同；未复制 C1 提交或操作其作者分支。
- 收到协调者合入通知后再次 fetch，读回 `origin/main=f15754129ac64a1dd02ec93abb00f9237bf0c9f8` 并成功 rebase；该提交为最终比较基线。
- 两个只读子代理分别调查在线字段使用、离线缓存生命周期；主代理核对实际调用并形成此表。

## 全部 App 字段

“主机独有”表示每个来源独立持有，包括根实例承担的本机业务。  
“共享”仅指在线实例的同一根库 Arc，不表示其他字段跨主机共享。  
表中全部字段均由 `src/app.rs:64` 的 `App::new` 创建。  
在线调用在 `src/remote/instances.rs:21`，离线调用在 `src/remote/cache.rs:54`。  
只有在线 `mapping_store` 在构造返回后改为根库 Arc。

| 字段 | 在线构造 | 离线缓存构造 | 实际使用者 | 归属和删除判断 |
| --- | --- | --- | --- | --- |
| `config` | 根配置克隆并覆盖目标字段 | 根配置克隆，清除 proxy/clock/binding | `App::connection`、目录读取、快照和来源门禁 | 主机独有，不能删 |
| `store` | `Store::open_configured` 打开子库，再导入旧根库远程记录 | 打开已存在且经普通文件检查的子库 | 会话保存、归档、`catalog::view::read` | 主机独有，不能删 |
| `mapping_store` | 先克隆子库 Arc，再替换为根库 Arc | 保持子库 Arc | 原生校准、探针过滤、`archive.rs:70` 的根 pending 门禁 | 在线共享；离线未启动相关业务，根快照另从根库过滤探针；不能统一改为子库 |
| `native_mapping_scope` | `None` | `None`，无实时校准 | `app/native_mapping.rs:53`、`native_backfill.rs:26`，重连时失效 | 主机独有，不能共享 |
| `refresh` | `false` | `false` | `observer.rs:454` 根历史 worker 消费；归档和校准也写目标主机信号 | 主机独有；远程当前无历史 worker，但共用操作仍写，不能直接删字段 |
| `remote_refresh` | `false` | `false` | 根 `remote_worker`，`observer.rs:565` | 根独有，仅能随类型拆分移出远程对象 |
| `catalog_refresh` | `true` | `true`，无 worker 消费 | 根调度 `app/remote.rs:49`，目标目录 worker 取走 | 主机独有，不能删在线实例字段 |
| `catalog` | 从主机子库加载快照及分页进度 | 同样加载，`live=false` | `catalog/view.rs:13`、`catalog/worker.rs:60` | 主机独有；删离线加载会丢目录项目和来源校验结果 |
| `health` | 默认未验证，再写 `hostLabel` | 默认未验证，再写标签、停用错误 | 观察 worker、`app/remote_snapshot.rs:23`、`catalog/view.rs:120` | 主机独有，离线仍需如实表达未验证 |
| `projects` | 空的本机项目目录 | 同左 | 根 `project_worker` 和根 `snapshot`；远程实际用 `catalog.snapshot.projects` | 根独有，仅能随类型拆分移出 |
| `title_lock` | 未占用 | 未占用 | `App::sync_title`、归档标题互斥 | 主机独有，不能跨主机合并 |
| `active_sessions` | `None`，后续运行扫描填写 | `None` | `remote/scans.rs:251`、`catalog/view.rs:130` 判定缓存新鲜度 | 主机独有，离线不得沿用实时集合 |
| `evidence_gaps` | 空 | 空 | `observer.rs:235` 运行证据；断线时清空 | 主机独有，不能跨来源保留 |
| `preview_turns` | 空 | 空，不标注实时预览版本 | 观察器记录、`app/preview.rs:50` 读取 | 主机独有，离线不得沿用实时版本 |
| `thread_operations` | 空，按需建锁 | 空 | `thread_lock`，会话保存、归档；根还锁 `connect:host` | 主机独有，同 UUID 不得跨主机串成一条业务 |
| `ssh_previews` | 空 | 空 | `app/ssh_setup.rs:50` 仅根入口可用 | 根独有，仅能随类型拆分移出 |
| `native_generation` | `0` | `0` | `app/native.rs:28`、`:65` 拒绝迟到的原生观察 | 主机独有，不能共享 |
| `enabled` | `true`，禁用主机随后停用 | `true`，安装前立即 `deactivate` 改为 `false` | 连接/写入门禁、目录新鲜度、关闭 worker | 主机独有，不能删离线停用 |
| `workers` | 空，安装时才启动目标 worker | 空，始终不启动 | `install_remote`、`shutdown`、替换实例前 join | 主机独有，空集合也参加关闭门禁 |
| `remotes` | 空 | 空 | 根安装/汇总主机；通用 `shutdown` 仍读取空集合 | 根独有业务，仅能随类型拆分移出 |
| `remote_requested` | 空 | 空 | 根手动连接、重试及新建路由 | 根独有，仅能随类型拆分移出 |
| `lifecycle` | 未占用 | 未占用，停用时获取写锁 | `worker_operation`、`deactivate` 排空在途操作 | 主机独有，不能删关闭边界 |

## 两条入口不是重复执行同一组覆盖

| 配置或动作 | 在线 `build_remote` | 离线 `install_cached_remote` | 保留原因 |
| --- | --- | --- | --- |
| `host_id`、`data_dir` | 当前登记目标及哈希子目录 | `source.json` 恢复主机及已核验目录 | 都需保持主机身份和独立库 |
| `auto_archive_seconds` | 使用显式远程间隔 | 保留根克隆值，但实例停用且不启动 worker | 不能让本机间隔自动启用远程归档 |
| `remote_retry` | 使用主机 `auto_connect` | 保留根克隆值，无实时 worker | 仅在线需要自动连接偏好 |
| `proxy_command`、`clock_command` | 为该主机生成命令 | 清空 | 离线读缓存不执行 SSH 或本机实时读取 |
| `remote_binding` | `Binding::prepare` | `None` | 在线核验物理来源，离线不伪造已确认连接 |
| `mapping_store` | 改用根库 | 保持子库 | 在线原生校准和单项归档需要根记录；离线汇总从根过滤 |
| 来源检查 | `verify_source` 在开子库前运行 | `cached_host` 检查来源记录、目录身份，数据库须普通文件 | 不能把离线加载变成远程登记或重绑定 |
| 旧根记录导入 | `import_remote` | 不导入 | 兼容导入只在线上按 `legacy_remote_import` 执行，不应随公共构造带入冷缓存 |
| 启动 | 安装时按 `host.enabled` 启动 `start_host` 及自动归档 | 停用后放入根 map，不启动 | `App::new` 本身不启动线程、不建立实时连接 |

在线有 7 项配置覆盖，离线有 5 项，实际共有的只有主机和目录两项赋值。  
在线和离线的标签更新相同，但其前后分别是旧记录导入、停用及受保护安装。  
提取这些赋值或标签更新只能搬走原行，并新增调用和参数，未发现可删除的完整初始化组。

## 断线、重连、重启和来源变化

1. `deactivate` 先置 `enabled=false`，再取得生命周期写锁，等待在途操作结束。
   它断开 binding、将目录 `live` 置为 false，并保存错误健康状态；不删除会话、备注或墓碑。
2. `attach_remote` 在代理相同，且实例已启用或目标继续禁用时复用。
   需要启用已停用实例或代理变化时，先 `shutdown` 再重建。
   `shutdown` 关闭登记、排空操作并 join worker；重建从同一子库加载，而不复用旧实时内存。
3. 冷启动 `snapshot` 和 `refresh_remote_instances` 都先调用 `load_remote_cache`。
   缓存加载不要求主机仍在登记中，不执行 SSH；`State::load` 恢复目录和分页进度。
   `append_managed_host` 和 `catalog::view::read` 同时检查实时健康和 enabled，旧记录保持只读。
4. SSH 生效主机、用户或端口变化时，`verify_source` 拒绝重绑定。
   `attach_remote` 收到此错误后停用旧实例，再返回错误。
   连接时 binding 继续校验 peer、UID、home 和 Codex home，保存及原记录保持。
   无效缓存单独记入 `remoteCacheErrors`，不隐藏其他主机或改写坏文件。
5. `legacy_mode` 仍使缓存加载直接返回，并拒绝远程刷新/显式连接。
   冻结模式不能因构造复用而连接远程；本次不改该分支。

## C1 依赖和续作入口

C1 `HostArchive` 明确借用 `root_batches: &Store` 和 `target_host: &App`。  
提交/核对仍从目标实例取连接、操作锁、线程锁、配置、刷新信号和子库。  
根库 claim/result 与目标库 confirm 的归属不要求改构造函数。  
`archive::request_with_writer` 的单项 pending 检查仍使用根 `mapping_store`。  
因此本次调查和不改代码的结论在 C1 合入前后相同。

本任务已按协调者通知 rebase 到包含 C1 的主线。  
若后续有实测依据决定拆根/主机类型，仍应 fetch/rebase 当时最新主线。  
需同时审阅 `HostArchive::target_host`、`archive::request_with_writer` 及其他 `&App` 调用，不能复制 C1 或只改构造点。  
这是一项后续迁移设计，不作为本次已有净删收益或当前阻塞。

## 验证及独立复核

所有回归针对本任务工作树，使用 `TMPDIR=/private/tmp`、锁定离线依赖和单测试线程。  
不新增测试、不接管 C1 测试，不改共享验收绑定，仅记录现有回归实际结果。

| 命令 | 实际结果 | 原始日志 |
| --- | --- | --- |
| `cargo test --locked --offline --lib remote:: -- --nocapture --test-threads=1` | 55 通过，0 失败，6.13 秒 | `/private/tmp/c2-remote-on-c1-tests.log` |
| `cargo test --locked --offline --lib catalog::view_tests:: -- --nocapture --test-threads=1` | 6 通过，0 失败，0.05 秒 | `/private/tmp/c2-catalog-view-on-c1-tests.log` |
| `cargo test --locked --offline --lib workers::app_tests:: -- --nocapture --test-threads=1` | 5 通过，0 失败，0.24 秒 | `/private/tmp/c2-workers-on-c1-tests.log` |
| `sh scripts/verify_context.sh` | `context check passed` | 本任务执行输出 |

表中为 rebase 后 C1 基线，三组共 66 项；耗时为运行时间，不含编译或审批等待。  
原 b859880 基线相同三组为 55/6/5 项通过，分别 8.21/0.15/0.44 秒。  
原通过日志为 `/private/tmp/c2-remote-baseline-tests-unrestricted.log`、`/private/tmp/c2-catalog-view-tests-unrestricted.log`、`/private/tmp/c2-workers-tests-unrestricted.log`。  
最初沙箱运行 remote 组为 19 通过、36 失败，目录视图组为 1 通过、5 失败，均报告 `Operation not permitted`。  
原日志保留在 `/private/tmp/c2-remote-baseline-tests.log` 和 `/private/tmp/c2-catalog-view-tests.log`。  
经授权在沙箱外运行同一源码后通过，没有修改断言、测试或业务实现。

`online_field_audit` 与 `cache_lifecycle_audit` 独立核对字段表和结论。  
两位均确认 22 字段分类、7/5 项覆盖、根映射库用途及无可独立净删组的判断。  
依据复核修正了停用实例仍禁用时可以复用、来源错误由 attach 调用停用两处描述。  
同时写明旧导入已有一次标记及冲突不覆盖保护，不能把省略导入归因为它会覆盖新记录。

最终仅提交本任务卡与结果记录，共 2 个上下文文件；准确提交头随最终回执提供。  
共享 STATE、journal、注册和归档由协调者处理，未推送、合入主线或部署。  
这些自动回归和文档检查不代替完整验收或真实客户端证据。
