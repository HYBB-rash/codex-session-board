# 待归档按钮交接

日期：2026-11-07，Asia/Shanghai。
初始源码基线 48ee1e16bae9399cab9a37967f9887a509d2a973、版本 0.1.29。
后续用户要求自主提交，已分五批提交至 0.1.34；未部署。
用户要求取消已完成界面状态，待归档卡片点击一次按钮即真实归档原会话。

已实现五状态界面、旧 completed 兼容投影、五状态分区配置和旧配置兼容、
独立归档 HTTP/MCP 动作、独立 DesktopConnection ArchiveWriter 权限。
原 Reader/TitleWriter 权限保持不变。请求校验状态版本、输入版本与未复核输入，
与标题、分区同步互斥。先分页检查真实归档列表，再预读原生 ID 和执行状态，
并直接核对原生最新轮次及用户消息 ID 与人工确认时的记录。
这覆盖本地观察器尚未落库但原生已有新一轮或同轮新输入的情形。
仅确认原生成功或归档列表明确包含目标时记录本地归档，清理队列并隐藏卡片。
旧扫描、后台观察和重启不会恢复已确认归档的卡片，备注和历史保留。
原生取消归档后自动恢复本地卡片尚未实现，文档已明确边界。

验证：

- cargo test --locked --offline --all-targets 的原 253 项通过，最终 10 项归档定向测试及统一验收覆盖当前完整的 256 项 Rust 测试。
- cargo clippy --all-targets --locked --offline -- -D warnings 通过。
- 统一完整验收最终复核 265 项，automated_passed=true，退出 2，release_ready=false。
- Chrome 实测五列、五项选择、按钮失败保留、成功移除、双击去重和窄屏按钮布局。
  浏览器归档回应用受控替身，不等于真实客户端归档证据。
- 隔离 Unix socket/WebSocket 端到端测试已覆盖真实 App 路由至归档角色协议。
- git diff --check 通过。
- 每批 ./scripts/check --pre-commit、严格 Clippy 和钩子测试通过，提交后再核对普通检查及干净工作区。

验收报告、Chrome 检查脚本、JSON 和截图位于：
/home/herman/.codex/visualizations/2026/11/06/01a1b1f2-98ba-7a32-9932-48d5856c766e/
原 0.1.29 报告为 archive-acceptance.json，对应实现指纹
edc1b73f0dba8c960f4ecc3c43f0bb6e3062abf24371848c6c49df00a7a693d1。
最终源码使用独立报告 archive-acceptance-0.1.34.json，不复用旧指纹。
archive-browser.json 记录未变更界面的隔离检查；浏览器归档回应用受控替身。

保留冻结 31 条需求、参考模型和锁；新增 Docs/ARCHIVING.md 说明本次显式变更。
STATE-001 的六状态与 BOUNDARY-001 的禁止真实归档不再是新界面契约，
不能借旧兼容测试通过签收新扩展。目标客户端实测仍待验证，不可宣称完整发布完成。

2026-11-07 用户要求提交全部本次改动，并将本地提交规则改为 Agent 自主决定。
已按每批 13 文件、400 行和补丁升版规则准备、审阅并提交，
每批运行 ./scripts/check --pre-commit；本机未找到 agent-git，沿用现有 Git 签名配置。
不能清理或忽略这些有用修改以使普通检查通过。
实际部署、原生分区撤除和用户真实会话归档需要对应授权。
后续安全入口是目标客户端验收与部署；本地提交不再需要逐次申请授权。

2026-11-07 合并收尾：0.1.35 整合 main 的 1978198 PWA 部署记录，保留双方历史。
日常版本应以该部署记录的 0.1.30 为准；PWA 安装入口已实测，归档扩展仍未部署。
新报告 archive-merge-0.1.35.json：265 项自动检查通过，完整验收退出 2，release_ready=false；
实现指纹 d0743bbfa654e2e101282853a85dc2efd6a6c7aa08c5fa806c9b14a74202fbe3。
