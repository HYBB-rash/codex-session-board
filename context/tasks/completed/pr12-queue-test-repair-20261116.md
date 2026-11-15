# PR12 打开会话测试竞态修复

owner: 01a1da86-d345-7f73-aa75-70fa97d46d41
execution_mode: isolated_write
allowed_write_set: scripts/check-light-ui.mjs, scripts/check-light-ui-open-wait.mjs, tests/ui_layout.rs, acceptance/bindings.json, context/tasks/active/pr12-queue-test-repair-20261116.md
base_commit: 4e2a26151fc45598274725c463017a26fd0f9773
worktree_path: /Users/hongyong/.codex/worktrees/pr-queue-author12-20261116/codex-session-board
coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
status: completed

反馈编号 ebdbd0ff-9e99-46de-83df-7e9cc934c301。开始时固定 HEAD 与分支符合反馈，
工作区干净。只修复轻量界面检查的完成条件及必要独立回归，不修改业务源码、
版本、共享状态、需求、参考模型和锁，不推送、不操作日用客户端。

原检查在点击打开后等待 !saving；业务 openSession 只维护 opening。
独立夹具预加载原检查脚本，扣住请求处理与响应完成两个阶段，通过真实 CDP
条件求值结果再逐次放行，不用固定 sleep 或重试掩盖失败。保留完整 writes 断言。

证据与最终 ready.json、result.json 位于源码外：
/Users/hongyong/.codex/visualizations/2026/11/13/01a1d38b-1ef8-7392-ba78-caa7935c5ab4/pr-queue-20261115/author-12

先保存原同步逻辑的失败、命令、退出码、源码头及补丁，再提交修复。
提交后先写 ready.json，再对冻结头执行定向 Rust 测试及独立延迟回归。
所有 cargo/check/validate 继承 CARGO_BUILD_JOBS=2。最终组合 CI 与独立审阅由协调者安排。

基线复现已保存 baseline-delay.json、baseline-delay.stdout.log、
baseline-delay.stderr.log、baseline.patch：原脚本第 96 行完整 writes 断言真实失败，
退出码 1，耗时 1.240 秒。请求处理被扣住时，实际 !saving 求值为 true，
写入记录仍为空。修复仅改为等待 !opening，保留原 80 个断言及完整写入校验。
独立夹具不改变原条件、CDP 返回值或断言参数；阻断期必须观察到条件为 false，
才能放行对应阶段，并核对完整写入断言发生在响应完成之后。
进一步记录 CDP 求值的发出阶段与收到结果的阶段，两者须相同才能放行。
同一最终夹具再次在未改原脚本上确定性失败，证据为 baseline-controlled.json
及对应日志、补丁，退出码 1，耗时 1.201 秒。
后续冻结头、逐项耗时、退出码与结论统一写入源码外 result.json，不自签独立审阅。

01fee032 的独立审阅指出新夹具没有常规调用者，该头暂未通过。
用户追加授权修改 tests/ui_layout.rs 和 acceptance/bindings.json 中必要内容。
新增 Rust 测试直接调用双阶段夹具并检查成功报告、请求与响应阶段等待结果，
绑定至 NAV-001 的 common 列表，覆盖 macOS 与 Linux 的常规验收发现路径。
原脚本 80 项检查、原 Rust 测试及其全部断言不改。新提交立即更新 ready.json，
随后以 ui_layout 整个常规 Rust 测试入口验证调用，不以手工 Node 单脚本代替。
基线失败证据与已审阅的等待修复、独立夹具保留，最终组合 CI 仍由协调者执行。

最终头64dc948ad6c47635aaf71e1854beb6765cc43013经独立审阅通过。
常规ui_layout组实跑24.242秒通过，清单与绑定审计通过；随0.3.93、
61d9d280进入真实主线，CI43746211763双平台组合及实际包启动通过。
原会话修复工作树保留，未安装或重载日用插件。
