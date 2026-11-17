# 项目操作准入修复 PR 作者任务

owner: codex
execution_mode: isolated_write
allowed_write_set: 本任务记录、项目操作准入修复涉及的源码、测试与文档
base_commit: c397839004bb69e60ab6cd334fa2616faf5ef2a5
worktree_path: /home/herman/.codex/worktrees/project-boundaries-author/notion-program
coordinator: 当前 PR 作者会话
status: submitting

用户显式调用项目 pr-task 技能，授权本任务验证、推送、创建 PR、巡检及有限回修。
范围沿用项目操作准入修复，不包含合并、部署或真实客户端业务验收。

## 固定入口

- 作者分支为 `codex/project-action-boundaries-20261117`。
- 冻结检查器为
  `/home/herman/.codex/worktrees/project-boundaries-checker/notion-program`，
  固定在上面的 base_commit，平台为 Linux x86_64，供审阅及队列沿用。
- 私有状态目录为
  `/home/herman/.local/share/codex-session-board/pr-tasks/project-action-boundaries-20261117`。
- 绑定当前 CODEX_THREAD_ID。后台地址来自本会话实际祖先进程的 app-server
  启动参数，登记时另核对 socket 所有者、权限、对端启动身份及原会话实时状态。
- 当前会话已有调查与开发上下文，不登记自动归档。合并或关闭后的会话归档留待人工处理。
- 本机使用既有 GitHub CLI 登录，受信审阅账号显式登记 HYBB-rash。
  自动模式保持默认十分钟、三轮、七十二小时，不自行扩大预算。
- Linux 使用 CLI 的 no-schedule 交付与源码外独立 systemd 用户定时器，
  巡检只执行冻结检查器的 tick；终态只清理本任务的定时器，保留源码和工作树。

## 源码与证据

从干净的旧开发树迁出已提交的六个任务提交，基于当前远端 main 独立应用，
没有修改原开发树或重写其历史。移植无冲突，版本与共享 STATE 保持当前主线。
原修复范围和失败证据见
`context/tasks/completed/project-action-boundaries-20261117.md`。
该记录中的 992 项结果属于旧基线，不用于签收本 PR 的当前头。

交付由冻结检查器的 pr-task submit 执行当前头 submit 门禁并推送草稿 PR。
完整 PR CI、独立审阅、组合门禁、发布和目标环境验收分别判定。
PR 链接、准确头、验证报告、巡检与回修状态以私有 task.json 和 GitHub 为准。
