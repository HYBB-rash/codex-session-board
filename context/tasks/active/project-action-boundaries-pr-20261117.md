# 项目操作准入修复 PR 作者任务

owner: codex
execution_mode: isolated_write
allowed_write_set: 本任务记录、项目操作准入修复涉及的源码、测试与文档
base_commit: c397839004bb69e60ab6cd334fa2616faf5ef2a5
worktree_path: /home/herman/.codex/worktrees/project-boundaries-author/notion-program
coordinator: 当前 PR 作者会话
status: repairing

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

## 反馈回修 623747e1

本事件已由冻结 CLI 返回 claimed，原头为 681f68c189bbb0d77458253d0b0a537acee859a3。
受信 HYBB-rash 在 PR21 的反馈评论 6944099160 对应最终候选
43f3ae6d6eaa05bf790314e9c2707c138f1cfa65、CI 44054543905 attempt 1。
下载并核对 acceptance-linux-x86_64 artifact 14316141060 的来源与失败报告。
该候选相对原头只改了五份版本及冻结规则清单，业务源码与失败用例相同。

原失败和复现保存在私有状态目录的 `repair-623747e1` 下：
原头单例首次通过，随后仅给同一测试增加等待后台无效回包的诊断同步，
确定性得到“远程会话列表格式无效”和原 CI 的“当前不可修改”失败。
`diagnostic-ordering.patch` 与日志保留该调度改变，不把它冒充原树原样失败。
没有将该问题归为 runner 故障，也没有反复重跑获取通过。

根因是连接测试夹具默认对 thread/list 回答通用 ok，测试却手动注入发现和
runtimeVerified=true，与真实后台清除无效运行证据竞争。
修复只调整测试：正例提供合法会话及轮次列表，等待真实扫描完成、会话活动及
运行核验成功后再验证可保存；新增坏回包反例，等待真实失败后断言缓存只读、
数据库与桌面配置均不变。两例都关闭自己创建的运行实例并等待后台线程退出。
独立复审指出负例不能把初始“尚未连接”错误当作目标扫描失败，
等待条件已收紧到该次无效列表错误，避免产生另一个调度竞争。
生产只读及身份保护、库与 native_authority 两线程、断言强度和版本保持不变。
返回头仍由固定检查器 submit 验证推送，并由独立只读审阅者复核。
