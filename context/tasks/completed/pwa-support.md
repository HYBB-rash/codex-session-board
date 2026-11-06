# 网页 PWA 安装支持

owner: codex
execution_mode: isolated_write
allowed_write_set: Cargo.toml, Cargo.lock, src/http.rs, assets/, tests/board_io.rs, scripts/bind_rust_tests.py, acceptance/bindings.json, README.md, context/
base_commit: 836f734d095d7a308ef18959dbc7ebb1884e3eb0
worktree_path: /home/herman/.codex/worktrees/pwa-support/notion-program
coordinator: codex-pwa

用户明确要求使用独立工作树继续 PWA，不影响另一项提交整理任务。
原工作区有未提交内容，本轮按用户新指示从已提交 HEAD 创建干净工作树，
不复制原工作区的改动。原分支被占用，独立分支使用 codex/pwa-install。
保留冻结 31 条需求、参考模型与锁。

采用浏览器标准安装清单、现有 SVG 图标、安装入口和 Service Worker。
只给 HTTP 网页注入资源；MCP App 继续自包含。
Service Worker 只处理首页导航失败，不缓存页面令牌、会话或写入请求。
通过隔离 HTTP 实例、Chrome 安装资格检查及完整验收入口验证。
不操作现有部署、配置、数据或原工作区暂存区。

已实现清单、主题和图标、真实浏览器安装提示、独立窗口配置及服务未连接页。
HTTP 资产继续执行 Host 校验和 no-store；API 写入保护不变。
新增两个真实 HTTP 回归，并验证 MCP 资源不加载网页 PWA 组件。
严格 Clippy、格式、JS 语法、上下文检查通过。
完整验收的 230 项自动检查全过，退出 2，release_ready=false。
Chrome 156 独立配置及数据目录的专项检查通过，真实安装提示已出现。
实际系统安装、独立窗口启动和冻结契约的其他外部验收尚未签收。

2026-11-06 用户明确授权按仓库规则准备补丁版本并提交本批修改。
本轮只收尾 PWA 源码提交，不签收完整发布验收；补丁版本准备为 0.1.19。
提交规模、实际暂存内容和提交前检查按规则审阅，提交后执行普通检查。
新版本的浏览器证据与完整验收报告位置见本任务日志。

用户随后授权以 rebase 方式合回。初始提交 106fb6e 在目标 1452dd7
之后重放，按版本步进规则改为 0.1.24；原 0.1.19 报告只证明原提交。
目标已提交的状态分区实现和验收绑定保留，新基线验证另行记录。
等待项目筛选任务结束后，再重放到其最终提交 e896a1d，版本调整为 0.1.27。
随后原任务开始部署并修复真实环境中的项目归属问题，本轮继续等待。
待其在 e828e96 提交且工作区干净后再次重放，PWA 功能版本为 0.1.28，
完整保留已实测的部署修复及记录，不操作运行服务。
