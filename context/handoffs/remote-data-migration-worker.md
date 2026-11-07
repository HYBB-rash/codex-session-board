# remote-hosts 数据迁移补丁

- 状态：按协调者最终基线分六步签名交付；本记录随最后一步回滚测试提交。未部署、未执行真实数据迁移。
- 日期：2026-11-08。
- 工作目录：`/home/herman/.codex/worktrees/remote-data-migration/notion-program`。
- 原补丁 HEAD：`918ee9e4b3d3e3c1b3ab00dcb0f9425f2ce950dd`。
- 正式整合基线：`dd771a2b6300dabd112957fcbebeae223d14b27c`，v0.2.25。
- 分支：`codex/remote-data-migration-integration`；交付范围为该分支相对上述基线的六个线性提交，末端 SHA 由完成回复给出。
- owner: remote-data-migration-worker-successor
- execution_mode: isolated_write
- allowed_write_set: `src/service.rs`、`src/service/migration*`、本记录、四处版本及 `acceptance/bindings.json`。
- base_commit: `dd771a2b6300dabd112957fcbebeae223d14b27c`
- worktree_path: `/home/herman/.codex/worktrees/remote-data-migration/notion-program`
- coordinator: `/home/herman/.codex/worktrees/remote-parity-lab/notion-program`
- 接续上一 worker 的补丁，保留合法 alias 修复及职责拆分；未操作协调工作树、共享 STATE、任务注册、journal 或真实配置与数据。
- 切换前完整备份 15 个 tracked/untracked 文件，包括已被协调者整合的两份旧 fixture 文件。保留 stash `2177f850d271d335106de0efe6e2a3ebdf210274`，不 pop/drop。
- 仅恢复 13 个 migration 交付文件，恢复后逐一核对 SHA256 与备份完全一致。最终 12 个 Rust 文件亦全部与审阅补丁的哈希一致；本记录更新为正式交付说明。
- 备份副本及哈希清单在 `target/dev/migration-integration-backup/`。旧 fixture 未回灌，基线中的新 fixture 保持原字节。
- 签名逐命令使用 `/tmp/remote-parity-vault-sign.py`；验签使用任务目录内的临时公钥名单，不修改用户 Git 配置。

## 行为及边界

迁移入口仍使用原有目标目录锁与受管进程检查，数据库发布委托给独立 helper。按源磁盘的 `remote-hosts/<64位小写SHA256>/` 枚举全部持久主机，不以当前主机注册表或 enabled 状态过滤；注册表已删除、连接已禁用的主机数据仍会迁移。

主库及每个主机子库分别调用 SQLite Backup，保留已提交但尚未 checkpoint 的 WAL 内容。目标使用独立数据库文件；不直接复制源 WAL、SHM 或 journal。子库仅额外允许 `source.json`、`scan-progress.json` 两个 sidecar，按原字节保留，文件权限为 0600，新建目录为 0700。进程记录、socket、锁、日志、凭据文件等不从源复制。

先检查目标配置、主库及其 SQLite sidecar、全部目标主机目录的冲突，再在目标文件系统内的私有 `.migration-<uuid>` 目录准备完整备份。所有备份完成后，通过不可覆盖的 hard link 安装文件，最后发布配置；中途失败不会发布配置。主库缺失时仍迁移存在的远程子库，旧返回字段 `databaseCopied` 仍仅表示主库是否复制；新增 `snapshotConsistency="per_database"` 明示一致性范围。

显式源配置路径、源 data_dir、目标数据根和目标配置父目录沿用 `Config.resolve_path` 的别名语义。源配置文件本身的显式 symlink 也可解析，相对 data_dir 仍相对真实配置所在目录。源数据库只解析父目录，最终数据库文件不 canonicalize，避免掩盖根内逃逸链接。目标配置先用 symlink_metadata 拒绝既有最终项，再只解析父目录，取得规范路径后再次检查；发布使用固定的规范路径。

不再复制默认 XDG 环境路径的解析规则，也不对授权根之外的合法祖先链接执行 blanket no_symlinks。CLI 已经解析默认配置父目录的情况有正例；最终配置文件若已存在，仍拒绝迁移，dangling 配置链接也不能作为可写目标。

根内主库、remote-hosts、主机目录、数据库 SQLite sidecar、允许的 JSON sidecar 和 launch.lock 继续拒绝 symlink。源和目标的安全负例均覆盖通过合法根别名访问的情况。发布每个数据库前再次检查目标库及 WAL/SHM/journal，防止备份期间外部新增 SQLite 文件；文件最终安装仍使用不可覆盖的 hard link。

回滚按创建顺序逆序处理本次新建项，核对 dev/inode；外部替换过的文件保留并报错。目录仅在为空时删除，不递归删除目标主机目录或已有目标目录。测试验证外部新增文件、已有标记文件及失败前已有目录得到保留，并可重试。

一致性与恢复限制：

- 每个 SQLite Backup 是该库自身的一致备份；主库、不同主机库、JSON sidecar 不属于同一事务或同一时刻的快照。源持续写入时可能跨库时间错位；需要统一业务时间点时，应由部署操作协调源写入方暂停，本补丁不自动停止源服务。
- 保障正常返回错误时的进程内回滚，不承诺进程被杀或掉电后的事务恢复。配置最后发布，但多个数据文件不是一次原子替换；崩溃遗留文件需要人工审阅，不能直接覆盖重试。
- 静态路径与发布阶段均检查链接；这不构成对同 UID 恶意进程持续替换整个目录树的完整防御。
- 服务入口原有 `launch.lock` 是目标端正常运行锁，会创建或保留在目标目录；它不是从源复制的文件。失败回滚清理的是本次迁移事务的数据库、sidecar、目录；不删除运行锁以免破坏锁的 inode 协调。

## 测试及证据

定向测试在临时目录构造主库和两个不同哈希主机子库，保持 WAL 写入未 checkpoint；对比 sessions、changes、title_updates、settings、remote_sessions、remote_projects 的全部列。种子数据覆盖最新备注、历史备注、标题待重试队列、input_revision、selection_turn/users/selected_at、native revision/pending key、archived 墓碑及 legacy import 标记。source.json 和 scan-progress.json 验证逐字节复制。测试不连接真实主机，不迁移用户数据库。

新增 23 个 test IDs 按编译接线分批由既有 `scripts/bind_rust_tests.py` 登记至 DATA-002，保留生成器既有其他覆盖关系。以下为完整精确 ID：

```text
lib::codex_session_board::service::migration::tests::aliases::source_loading_accepts_configuration_and_data_aliases
lib::codex_session_board::service::migration::tests::aliases::migration_accepts_source_and_destination_root_aliases
lib::codex_session_board::service::migration::tests::aliases::migration_accepts_directory_ancestor_aliases_for_missing_targets
lib::codex_session_board::service::migration::tests::aliases::migration_accepts_default_configuration_parent_alias_after_cli_resolution
lib::codex_session_board::service::migration::tests::aliases::migration_accepts_relative_configuration_without_changing_current_directory
lib::codex_session_board::service::migration::tests::aliases::migration_lock_accepts_data_directory_alias
lib::codex_session_board::service::migration::tests::aliases::source_loading_rejects_dangling_configuration_or_data_aliases
lib::codex_session_board::service::migration::tests::aliases::migration_rejects_dangling_destination_root_or_parent_aliases
lib::codex_session_board::service::migration::tests::aliases::migration_rejects_final_configuration_symlinks_through_parent_alias
lib::codex_session_board::service::migration::tests::guards::migration_refuses_existing_root_or_host_data_before_publication
lib::codex_session_board::service::migration::tests::guards::migration_refuses_existing_sqlite_sidecars
lib::codex_session_board::service::migration::tests::guards::migration_rejects_source_symlinks_without_following_them
lib::codex_session_board::service::migration::tests::guards::migration_rejects_destination_symlinks_including_dangling_configuration
lib::codex_session_board::service::migration::tests::guards::migration_rejects_configuration_overlapping_database_or_sidecar
lib::codex_session_board::service::migration::tests::guards::migration_refuses_existing_configuration_before_creating_destination
lib::codex_session_board::service::migration::tests::guards::migration_refuses_runtime_lock_symlinks_before_opening_external_files
lib::codex_session_board::service::migration::tests::rollback::second_host_backup_failure_cleans_staging_and_allows_retry
lib::codex_session_board::service::migration::tests::rollback::configuration_failure_rolls_back_installed_files_and_preserves_external_additions
lib::codex_session_board::service::migration::tests::rollback::rollback_preserves_files_replaced_by_another_writer
lib::codex_session_board::service::migration::tests::rollback::installation_refuses_concurrent_target_entries_and_preserves_them
lib::codex_session_board::service::migration::tests::rollback::configuration_race_preserves_external_files_and_symlinks
lib::codex_session_board::service::migration::tests::snapshots::migration_preserves_root_and_all_host_wal_state_and_only_allowed_sidecars
lib::codex_session_board::service::migration::tests::snapshots::migration_copies_host_databases_when_the_main_database_is_absent
```

原有三个迁移回归测试的函数和断言原样保留，只调整 import 接线；在新基线的 service 全量测试中全部通过：

```text
lib::codex_session_board::service::tests::migration_uses_backup_and_preserves_source_history_and_pending_titles
lib::codex_session_board::service::tests::failed_backup_or_config_publication_cleans_only_the_new_database
lib::codex_session_board::service::tests::existing_database_is_preserved_even_when_source_is_absent
```

全量提交钩子还覆盖以下既有测试：

```text
lib::codex_session_board::config::tests::symlink_data_aliases_resolve_to_the_same_identity
example::remote_parity_lab::host_isolation_tests::host_re_registration_requires_explicit_migration
```

上一补丁中把授权源 root、目标 root、源配置及配置父目录 alias 当作非法路径的本地断言改为正例；其余负例保留，增加 main database、SHM、journal、悬空根/父目录、最终配置链接与并发占用的覆盖。新模块名改变了此前 15 个新增测试的完整 ID；不要沿用交接旧版中的 ID。冻结需求、参考模型、锁、既有验收断言均未修改。

每一步先正常准备 Cargo 版本，再同步两处 plugin 清单；四处版本一致。每步运行绑定生成器、选择暂存文件、审阅差异、执行 `./scripts/check --pre-commit`，再由 Git 钩子对暂存快照执行全部目标 Clippy 和完整 Cargo 测试。仅使用上述独立工作目录的环境：

```bash
export CARGO_TARGET_DIR="$PWD/target/dev"
export CARGO_BUILD_JOBS=4
export RUST_TEST_THREADS=4
cargo test --locked --offline --lib service::
cargo test --locked --offline migration
cargo clippy --locked --offline --all-targets -- -D warnings
rustfmt --check --edition 2024 --config skip_children=true src/service.rs src/service/migration.rs src/service/migration/*.rs src/service/migration/tests/*.rs
git diff --check
./scripts/check
sh scripts/verify_context.sh
python3 scripts/verify_acceptance.py --report "$CARGO_TARGET_DIR/migration-integration-logs/final-acceptance.json"
```

第六步提交前已实际执行的定向结果：

- service 全量：35 passed / 0 failed / 0 ignored，包含新增迁移测试 23 项和既有 service 测试 12 项；其他 114 项库测试由全量提交钩子覆盖。
- 跨测试目标 migration 筛选：库测试 24 项加 remote_parity_lab 示例 1 项，共 25 passed / 0 failed / 0 ignored。其他目标没有匹配项，仅表示本次筛选未运行，不能据此声称这些目标全量通过。两个名称不含 migration 的既有迁移负例由完整库测试覆盖。
- 全目标 clippy：退出 0，无警告。
- 指定 Rust 文件 rustfmt 检查：退出 0。
- `git diff --check`：退出 0；未跟踪 Rust 文件另外由 rustfmt 检查覆盖。
- 最终生成器清单：439 个 Rust 原生测试，加 26 个 Python 验收器测试，共 465 个不同 ID。基线的 442 个 ID 及其全部旧覆盖关系原序保留，恰好新增 23 个不同 ID。
- 冻结的 31 条需求、参考模型、锁、生成器及协调者两份 fixture 的 SHA256 与基线一致。
- 六次提交的门禁、完整测试与验签记录位于 `target/dev/migration-integration-logs/01-*` 至 `06-*`。最后一次完整测试及提交后清洁检查，以对应日志和完成回复的实际结果为准。
- 完整验收仍须目标环境证据；外部缺证应如实返回 2、保留 `release_ready=false`，不得冒充发布验收通过。
- 仅使用本工作树 `target/dev`；没有使用协调者 target，没有运行真实迁移或部署。

## Diff 与整合

代码差异：`src/service.rs` 为 +13/-49；新增实现共 414 行，新增测试共 830 行。本记录以外共 12 个 Rust 文件，合计 13 个交付文件，没有新增依赖。下表行数按 rustfmt 后统计：

| 文件 | 行数 | 职责 |
| --- | ---: | --- |
| src/service.rs | +13/-49 | 迁移入口与既有服务锁、进程核验接线 |
| src/service/migration.rs | 60 | 规范路径与备份、安装、最后发布配置的编排 |
| src/service/migration/paths.rs | 87 | 授权根解析、叶项冲突、根内链接检查和迁移锁 |
| src/service/migration/database.rs | 42 | SQLite Backup、目标预留和失败清理 |
| src/service/migration/plan.rs | 151 | 磁盘主机枚举、允许项、冲突校验和安装 |
| src/service/migration/transaction.rs | 74 | 私有 stage、已创建 inode 账目与回滚 |
| src/service/migration/tests.rs | 5 | 按职责注册测试模块 |
| src/service/migration/tests/fixture.rs | 187 | WAL、状态历史、队列、主机库和字节快照夹具 |
| src/service/migration/tests/snapshots.rs | 60 | 全量持久数据、WAL 与允许 sidecar 的保真 |
| src/service/migration/tests/guards.rs | 227 | 既有目标、根内链接、路径重叠和锁保护 |
| src/service/migration/tests/aliases.rs | 216 | 合法根/祖先/配置别名与悬空、最终链接边界 |
| src/service/migration/tests/rollback.rs | 135 | 备份、配置失败、外部新增/替换与重试 |

正式整合顺序如下。前五步的行数含四处版本及 binding，按新增加删除统计；第六步的最终规模由 `06-precheck.log` 核定，不允许超过 400 行或 13 文件：

| 步骤 | 版本 | 提交 | 内容 | 文件 / 行 | 原生编译清单 |
| --- | --- | --- | --- | --- | --- |
| 1 | 0.2.26 | 379d34a | 授权根解析与 SQLite 单库 helper 提取 | 8 / 189 | 416 |
| 2 | 0.2.27 | b370a95 | 全主机计划、stage、发布与回滚实现 | 9 / 306 | 416 |
| 3 | 0.2.28 | fd9b550 | WAL 与允许 sidecar 保真，登记 2 例 | 9 / 268 | 418 |
| 4 | 0.2.29 | 864984d | 冲突与根内 symlink 保护，登记 7 例 | 7 / 254 | 425 |
| 5 | 0.2.30 | b94beea | 授权 alias 正例与最终项负例，登记 9 例 | 7 / 247 | 434 |
| 6 | 0.2.31 | 本记录所在提交 | 回滚、并发文件与重试，登记 5 例及交接 | 以门禁为准 | 439 |

协调者可对上述基线至该分支末端执行 fast-forward，保留六提交顺序。没有重新接线旧 fixture，也没有覆盖新基线的连接保护。

最终交付共 18 个路径：上表列出的 12 个 Rust 文件、本记录、`Cargo.toml`、`Cargo.lock`、`plugin.json`、`.codex-plugin/plugin.json` 及 `acceptance/bindings.json`。完整路径、提交 SHA、规模、测试数、哈希核对结果及最终检查日志由本树 `target/dev/migration-integration-logs/summary.json` 汇总。协调者继续拥有任务归档、STATE、journal、真实环境验收与部署。
