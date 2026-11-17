# 龙娘反馈的 Rust fixture 修复与 PR

owner: codex
execution_mode: isolated_write
allowed_write_set: scripts/dev_environment/runtime.py, scripts/dev_environment/configuration.py, acceptance/test_dev_environment.py, acceptance/bindings.json, scripts/check-transfer-feedback.mjs, scripts/check-dragon-fixture.mjs, scripts/check-dev-isolation.mjs, scripts/check-blocked-optional.mjs, tests/ui_layout.rs, Docs/MAC-DEVELOPMENT.md, Docs/DRAGON-FEEDBACK.md, context/tasks/active/dragon-fixture-pr-20261117.md, context/handoffs/dragon-fixture-pr-20261117.md, context/results/dragon-fixture-pr-20261117.md
base_commit: b66656a
worktree_path: /home/herman/.codex/worktrees/dragon-fixture-author/notion-program
coordinator: 当前会话

## 授权与交付

用户要求继续排查 Rust fixture 卡片只读，修好后完成实际拖动及动画停留时间的
隔离验证，并显式调用 pr-task 提交 PR。包含前序已确认的龙娘动画实现。
不修改正式插件、日用客户端、真实会话或部署配置，不合并或部署。
开发保持主线 0.4.3，不修改共享 STATE、需求、参考模型或验收锁。

作者树基于已刷新 origin/main，带入 1d9c0aa、82bb49a、1116fe1 三个本会话
提交，分别成为 6589a7c、120a1b8、392ba61。没有带入已单独合并的旧项目修复。

冻结检查器复用 PR21 队列的 Linux x86_64 版本 c397839：
/home/herman/.codex/worktrees/project-boundaries-checker/notion-program。
固定入口为该目录的 scripts/pr-task。保留该检查器及作者树。
私有任务状态：
/home/herman/.local/share/codex-session-board/pr-tasks/dragon-fixture-feedback-20261117。
当前原会话另有前序工作，不登记自动归档；审阅账号 HYBB-rash 保留。
凭据复用已有单一 GitHub 适配器，不存放秘密值。

## 首轮调查

只读保护来自 native_sections 下尚无字符串 sectionVersion。
历史发现、运行观察和原生位置读取并发，nativeVerified 的全局健康状态
不保证刚发现的每张卡片已经得到原生位置，先采样区分启动竞态与持续错误。
现 dev probe 只检查全局健康和项目目录，缺少卡片可操作就绪条件。

另已定位重启问题：真实程序模式会创建 mcp.sock，
开发入口的受保护启动清理只列出 supervisor、desktop 和 sidebar，遗漏 MCP。
沿用现有属主、文件类型及活监听者检查修复，不直接强制删除未知 socket。

未修改实现的实际采样已确认：入口返回后首轮三张卡片 sectionVersion=null、
readOnly=true，nativeVerified=true；588.38ms 后三张卡片均有字符串位置且恢复可写。
原生读取和历史发现的启动竞态成立，不需要修改生产只读保护或原生状态逻辑。
原始证据：前序证据根目录下 fixture-repair/before-samples.json。
当前修复开发 probe 的可写就绪条件，启动和停止均核对并清理三类 fixture socket，
增加四项 Python 回归并按原需求绑定，不更改需求契约。
14 项 DevEnvironmentTests 定向回归及上下文校验通过。
原生健康、只有位置、只有可写标记、远端记录和空列表均不能冒充本机可操作就绪；
MCP 失效路径能清理，活监听者、非 socket 和符号链接保持受保护。

## 验证计划

保存未修改实现的启动采样、只读原因和停止结果。
补充开发就绪、MCP socket 和停止后重启回归，维持只读及连接边界。
用真实 Rust 服务、私有数据库、原生依赖 fixture 和独立浏览器执行指针拖动，
核对保存前后位置、成功画面实际停留、连续操作与立即收起，再验证重启持久性。
通过固定检查器 submit 验证、推送并提 PR，核对准确头 CI 与监控状态。
