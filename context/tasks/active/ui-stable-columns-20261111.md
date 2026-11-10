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
正式部署和主线合并尚未执行。
