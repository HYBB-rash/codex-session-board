# 远程主机目录实现与 PR 交付

- owner: root
- coordinator: root
- execution_mode: isolated_write
- base_commit: d4b40bb3aae98569a310f1e412defbb1f20dddbb
- code_head: 8b4224ce15bf9ad0fb2e6d8d354d7eabd457ed2f
- worktree_path: /Users/hongyong/Developer/codex-session-board-remote-catalog-impl-20261114
- PR: https://github.com/HYBB-rash/codex-session-board/pull/11

## 交付行为

切换已登记、已连接的主机后，会话看板读取该主机全部当前项目、普通活动及归档历史，
并补入目标桌面已发布的置顶与自定义分组引用，不要求在操作机重建项目。
普通历史保持目标原生范围，全部 provider 的查询参数固定；空项目、旧迁移归属、
无项目会话及缺少可靠关联的引用分别保留，不能按 cwd 猜测归属或静默丢弃。

读取统一经过 App::connection 的 CatalogReader。RPC 参数和目标状态文件路径受限，
不读取会话正文、不刷新账户令牌、不新增项目写能力。目标账户、来源或生命周期变化
会拒绝发布过期结果；离线缓存和未核验来源只读。筛选不隐式连接其他后台。
界面提供主机、项目、活动/归档/全部筛选，以及采集时间、覆盖范围、错误与未解析项。

实现边界与取证设计见 Docs/REMOTE-CATALOG-DESIGN-20261114.md。
这是会话看板的目录能力；原生未发布内存草稿不能完整跨机重建，
未知接口版本或不完整证据明确显示 partial，不宣称已穷尽。

## 已验证结果

| 验证 | 结果 |
| --- | --- |
| 默认 `python3 scripts/verify_acceptance.py` | 770/770 自动测试通过，625 项原生测试及 145 项 Python 测试 |
| 31 条需求 | 17 pass、14 pending；缺少目标客户端证据 |
| 完整验收判定 | automated_passed=true、passed=false、release_ready=false，退出 2 |
| 全目标 Clippy | `cargo clippy --all-targets --locked --offline -- -D warnings` 通过 |
| 隔离 Mac fixture 联调 | 10/10 通过；服务与 fixture 已停止，cleanup_confirmed=true |
| 目录浏览器回归 | 包含在全量 UI 驱动测试中，新目录脚本 43 个断言通过 |
| 权限测试同步回归 | 6 项通过，竞态用例连续 10 次通过，完整拒绝日志断言保留 |
| 既有并发选主回归 | 单项连续 10 次通过，最终全量也通过 |
| 独立只读审查 | 无阻塞；同步屏障及权限断言强度另行复核通过 |
| 代码头 GitHub CI | Check PR changes，run 43626508528，changes 成功，head 精确匹配 code_head |

完整验收报告的 build_sha256 为
`c27416ac7dce6f8d72bbf9e7c4ffb1706086b9f51780fef78363720383539003`，
contract_sha256 为
`b8ffcf1ded7d8e6bd7607979172843ed069708e62281e54d64445d42ce0c3c3a`。
保持原 31 条需求、独立参考模型及锁，没有降低断言、跳过失败或用旧实现签收。

第二轮全量曾出现既有四进程选主用例失败，日志只能定位到启动 HTTP 身份就绪窗口，
尚未确定具体耗时阶段。选主路径与主线一致，本 PR 未改选主协议。
后续连续单项及最终全量通过，不代表该既有启动窗口的根因已修复；保留失败日志供后续调查。
另一个目录权限用例的异步 initialized 日志竞态，已通过安全读取回执形成同步屏障修复，
没有过滤日志或放宽拒绝请求断言。

## 证据与结束门禁

证据目录：
`/Users/hongyong/.codex/visualizations/2026/11/14/01a1da30-d415-7413-aa72-bf742f3110af/remote-catalog-20261114/implementation-20261115/`

- acceptance-release-final.json / .log：最终冻结源码的完整验收。
- acceptance-release.json、acceptance-release-fixed.json 及对应日志：前两轮失败记录。
- clippy-sync.log、catalog-guard-sync*.log、plugin-owner-repeat-*.log：专项验证。
- blocked-optional-final.json、browser-final.log、dev-stop-final.json：最新隔离验证及清理回执。
- github-check-runs-code.json：代码头 8b4224c 的全部 check runs，1/1 成功。
- project-check-final.log：归档提交后的 `./scripts/check` 结束门禁。
- github-pr-final.json、github-check-runs-final.json、github-status-final.json：
  最终 PR head、全部 check runs 与 legacy statuses 回执。

上下文归档不进入验收源码指纹。最终交付核对当前指纹仍等于报告，
PR head 等于最后推送的提交，所有实际检查成功；不能仅凭旧代码头 CI 通过交付。
未修改版本清单和共享 STATE；没有合并、发布、安装或重载日用插件。
真实 Mac/Fedora 目录差集、原生打开和既有写动作的目标环境验收属于后续独立阶段。
