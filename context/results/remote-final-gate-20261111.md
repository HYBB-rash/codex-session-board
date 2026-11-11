# Final gate 89

owner: codex
execution_mode: isolated_write
base_commit: 9a59d76a7000cc9a10a2450c5d02b01e0e69b1ee
worktree_path: /home/herman/.codex/worktrees/remote-final-gate/notion-program
coordinator: remote-final-integration-20261111

创建前确认协调树HEAD为base且status为空；通过Codex app create_worktree创建，分支codex/remote-final-gate。
仅修改门禁、其测试、GATE-001追加、四版本清单、本记录和额外授权的两份脚本。
未修改STATE、任务登记或journal，未操作SSH/看板MCP/CUA/会话/服务/模型/插件。

移植C4f的四个远端校验函数及调用、两个fixture/helper、九项原测试。
保留S的states参数链、section_preflight、mode fixture和全部18项原测试断言。
path_references枚举所有path引用以保护报告；references继续筛选含SHA的引用作全局完整性验证。
read_sha256_artifact强制64位小写十六进制SHA，供预检及remote_artifact共用，保留全局hash/review检查。
新增3项完整main/argparse回归：两Root缺/假SHA；六类输入的24种自身/规范化/hardlink/symlink保护；
五类权威附件的旧SHA待验证、旧review拒绝、更新review恢复正向。均为私有TemporaryDirectory夹具。

red日志保留30测试/40个预期失败；修复后全量Python unittest 40项通过，无跳过。
静态集合506+9+3=518；GATE-001为124，其他bindings映射及1800预算逐项未变。
两脚本精确移植S5571ff4：Chromium清理重试、生成器1800及build.rs指纹路径；不合并S记录。
版本0.2.88至0.2.89。原始red源、进程退出及日志均在outputs/remote-final-gate-20261111。
完整默认并行提交钩子、提交SHA及提交后check/context的最终记录见该目录commit及post-commit日志和最终回报。
此为门禁子任务交付，不签最终业务31/16、原生UI或部署；保留本工作树供协调者合并。
