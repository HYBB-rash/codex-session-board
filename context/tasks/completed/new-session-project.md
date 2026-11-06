# 新建会话使用明确选中的项目

owner: codex
execution_mode: serial_write
allowed_write_set: index.html, src/app.rs, src/projects.rs, src/mcp.rs, tests/board_io.rs, acceptance/bindings.json, Docs/PROJECT-FILTERS.md, context/
base_commit: 1978198aa4304e56237ac7b5c7c6c392e095ace0
worktree_path: /home/herman/.codex/worktrees/99a4/notion-program
coordinator: codex

用户确认仅处理页面已明确选择具体项目的情况。
搜索、全部项目、默认工作区和没有明确项目时保持现有新建行为，
不增加选择弹窗，不推断是否需要项目。
沿用已完成调研中的原生项目深链；同步 HTTP/MCP 参数并增加回归验证。
保留上一轮未提交的调研记录，不提交、不部署、不改变真实客户端选择。

## 实现和独立审查

前端仅在 filterMode=project 且选择值为 project:<id> 时发送 projectId。
其他状态继续发送空对象，后端不访问项目登记表，使用原来的 threads/new。
明确项目使用 codex://new 的 projectId 参数，以标准 URL 查询编码生成。
MCP board_new schema 同步支持可选字符串，HTTP 和 MCP 共用处理。

新建校验直接读取并解析本次独立登记表快照，不复用后台展示缓存，
避免后台刷新旧数据覆盖请求校验。项目移除、读取失败和非法类型明确报错。
测试捕获实际启动参数，覆盖 A/B 同名项目、特殊字符、状态保留、
有效旧缓存与失效登记表，以及默认入口不依赖登记表可用性。
MCP 回归经过真实 HTTP 服务验证可选参数转发。

独立代理执行实际前端 onclick、筛选和项目切换代码的 Node VM 验证，
18/18 场景通过，包括搜索隐藏选择、all/default、请求中切换与重复点击、错误恢复。
另一代理提出刷新校验竞态和旧缓存测试缺口，均已修正。
严格 Clippy、修改模块的 rustfmt 与 git diff --check 已通过。
全仓 cargo fmt --check 仍报告 HEAD 原有的 app.rs snapshot 排版及 lib.rs 模块顺序；
通过 git show HEAD 对照确认，不修改无关格式。
首次中间报告因审查修正发生在验证期间而失效，不能作为最终代码通过的证据。

验收发现新增 MCP 测试直接调用 backend 会让启动器把测试二进制当成服务启动。
已改为复用真实 MCP 子进程调用夹具，验证成功、默认入口和失效项目 isError。
相关三个 MCP 测试重新执行通过，严格 Clippy 与格式检查再次通过，
完整验收已在最终源码上重新运行。

## 最终验证与交付边界

python3 scripts/verify_acceptance.py --report /tmp/session-board-new-project-final-acceptance.json
退出 2，257 项自动检查全部通过，automated_passed=true。
缺少与当前实现匹配的真实环境证据，release_ready=false。
本记录归档代码实现与自动验证，不代表完整发布验收完成。
未实际打开新建页面验证原生选择、未部署、未提交、未升版。
工作区保留上一轮调研与本轮实现记录。
