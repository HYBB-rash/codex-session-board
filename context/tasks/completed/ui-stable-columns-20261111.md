# 修复筛选项目导致看板列宽变化并部署

owner: codex-ui-layout
execution_mode: isolated_write
allowed_write_set: index.html, tests/ui_layout.rs, scripts/check-ui-layout.mjs, acceptance/bindings.json, Cargo.toml, Cargo.lock, README.md, context/, outputs/, 正式看板部署包与插件缓存、marketplace、受管服务、已完成任务的worktree
base_commit: 1fc107dd161efe5c350a8199dfd5d1b8a183e63a
worktree_path: /home/herman/.codex/worktrees/ui-stable-columns/notion-program
coordinator: codex-ui-layout

用户授权创建隔离 worktree，修复上一轮确认的布局回归，验证后合并主线并部署上线，
最后清理已经结束任务的 worktree。没有推送授权。
主线存在其他任务未提交内容；从干净的 remote-parity-lab 检出读取主线已提交基点，
通过 create_worktree 新建本任务 worktree，不搬动或暂存主线的其他修改。
初始基点 1a86c73 缺少版本步进，提交检查拒绝通过。
已采用另一任务完成并通过提交检查的等价修复提交 1fc107d 作为基点，
在本 worktree 保全并恢复全部本任务修改，没有改写主线。

修复看板内容驱动最小宽度与标题按钮的禁止换行。
新增真实浏览器回归，验证长标题、长项目标签、项目筛选往返、搜索、五/六状态和窄屏横滚。
只读子代理分别核查部署/回滚步骤和旧 worktree 的任务、提交、产物及进程状态。
保留正式配置、用户库和备注；部署前备份并核验进程身份，部署后核对版本、内容及真实布局。
完整验收按原门槛执行；不修改冻结需求、模型或锁，不用自动通过冒充外部证据。

## 实现与专项验证

已改正原有 `.board` 最小宽度与标题按钮换行规则，没有增加项目分支或备用布局。
新增 Node.js 标准库与 Chrome CDP 浏览器检查，绑定 UI-003 原需求，不替代外部验收。
20 组五/六状态、长标题/路径/备注/项目标签、筛选往返、空搜索、801/800/600/390px 检查通过。
同一检查对旧页面失败，原因为长标题未在卡片内换行。
Rust `ui_layout` 专项通过；浏览器截图及测量位于 `outputs/ui-stable-columns/browser/`。
## 合并、发布与实屏核对

布局提交 fb9b697 已与主线 013a620 合并为 2caa1c6784eb8bf478f1e3b6c2cf30cc2f3de387，
正式候选为 0.2.82。主线另一项调查的未提交状态通过 Git autostash 保全，没有纳入本次提交。
Clippy、完整提交钩子测试、项目检查和上下文检查通过。
合并候选在独立构建目录执行完整验收，500/500 自动测试通过，14 条真实环境证据仍待补，
退出 2、release_ready=false；没有修改冻结契约、降低断言或复用失效测试二进制签收。

正式 HTTP 服务与正式插件已切换至 0.2.82，发布目录为：
`/home/herman/.local/share/codex-session-board/releases/0.2.82`。
二进制 SHA256 为 `d788037777849f12f2ec1365c90c61222738e9e290cbe231a18e416f244faada`。
服务使用原配置、原数据目录，configId 仍为 ce0664890c3b6c81。
部署前用 SQLite Backup API 备份五个库；部署后核对完整性、原会话 ID 集合及备注摘要，
配置文件逐字节不变。旧版发布包及备份保留用于回滚。

通过当前桌面 app-server 重载正式 MCP 服务，并从服务读取真实页面资源，
确认版本 0.2.82 及两条修复规则，证据为发布目录中的 native-reload-verification.json。
另用正式 HTTP 页面执行“全部项目 → Notion任务看板管理 → 全部项目”，
1280px 视口下 159 条与单项目 5 条会话均保持五列各 240px，页面整体没有横向溢出。
已保存测量 JSON 和两张正式页面截图，没有修改会话状态或备注。
当前桌面正展示其他任务的验收页面，本次没有切换或操作它，
因此本任务不将独立 HTTP 页面截图当作原生宿主实屏验收。
原生入口证据限于正式 MCP 重载及其真实资源；全局外部验收仍保持待补。

## 工作区与材料收尾

本任务完整输出已转存至主仓库 `outputs/ui-stable-columns/`，正式发布目录另保留
最终包、完整验收报告、布局回归和上线核对材料。运行服务及插件均不依赖本 worktree。
收尾提交合并并通过检查后，使用当前会话的 archive_worktree 保存可恢复快照并移除本工作区；
实际工具回执与最终检查结果追加到主仓库输出目录，避免为归档动作再制造新工作区。

历史 remote-connection-identity、remote-data-migration 和 remote-parity-lab 虽然已有阶段成果，
但三个工作区均关联仍 active 的远程验收会话 01a1b4de-ab7c-7841-bd77-3c0384dde49a，
其中 remote-parity-lab 还被验证 marketplace 引用；本轮保留，未把阶段完成视为所属任务已结束。
源码、未提交差异、Git 历史、忽略的实验输出及必要 target 原始记录已另存并核对摘要，
位于 `/home/herman/.local/share/codex-session-board/worktree-archives/20261111/`。
remote-restore-acceptance、state-section-consistency 仍在工作，九个历史 notion-exp-* 缺少完成及所有权证据，
均保持原样。没有推送代码，没有归档或删除其他聊天。

实际归档调用被 Codex 拒绝：`The task's primary checkout cannot be removed.`。
当前任务不能自行 handoff，因此本次 worktree 尚未移除，没有绕过主工作区保护。
修复、合并、发布与验证已完成；剩余清理独立登记于
`context/tasks/active/ui-worktree-cleanup-20261111.md`，交接说明位于对应 handoff。
