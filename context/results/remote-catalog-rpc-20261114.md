# CatalogReader RPC 权限

- owner: upstream_interfaces
- coordinator: root
- execution_mode: isolated_write
- base_commit: d4b40bb3aae98569a310f1e412defbb1f20dddbb
- worktree_path: /Users/hongyong/.codex/worktrees/remote-catalog-rpc-20261114
- allowed_write_set: src/rpc.rs、src/rpc/catalog_guard.rs、src/rpc/*tests*、src/rpc/framing.rs 的 CatalogReader 分支、scripts/remote_turn_proxy.py、scripts/test_remote_turn_proxy.py、本记录

设计读取自独立设计树 Docs/REMOTE-CATALOG-DESIGN-20261114.md；最新 origin/main 尚未包含该设计文件。

公开接口 `DesktopConnection::catalog_source(&self) -> Result<Value>` 仅对 CatalogReader 可用，
返回 codexHome、userAgent、version、peerFingerprint。version 在旧初始化未提供 userAgent 时为 null；
调用者按能力探测处理，不能用 null 推导支持。初始化字段先保存，来源核验后才绑定并开放读取。
远程 fingerprint 取已核验 SSH endpoint 与 Peer；本机取已核验 LocalAuthority 和 canonical CODEX_HOME。
本机文件根从本进程 HOME/CODEX_HOME 生成，核对初始化结果与目录所有者；不会信任前端任意目录。

thread/list 强制 useStateDbOnly=true；thread/read 强制 includeTurns=false；
account/read 强制 refreshToken=false。两个 fs 方法仅准已绑定 codexHome/.codex-global-state.json。
新角色仍走现有已登记主机、身份前导帧、Binding 和初始化验证。代理 catalog 模式保持响应正文，
不套用轮次压缩。原 Observer 压缩和写角色传输不变。未操作日用客户端、配置或会话。

独立 target /private/tmp/codex-catalog-rpc-target-20261114 中完成：
CatalogGuard 六项新测试、summary 两项、错误后台一项、framing 十项全部通过；
Python 四项通过；cargo clippy --lib --no-deps -- -D warnings 通过。未运行全量测试或全目标 Clippy。
提交与结束门禁以交付回执为准。完整组合测试由协调者统一运行，不将本记录视作发布或真实环境验收。

协调者需把新增 rpc::catalog_guard::tests 六项绑定到验收；本代理未越权修改 acceptance/bindings.json。
