# 插件统一持有业务运行时

owner: codex
execution_mode: coordinated_isolated
allowed_write_set: src/, tests/, scripts/, acceptance/bindings.json, acceptance/test_plugin_update.py, Docs/, context/, 四份版本清单, outputs/, 已授权插件部署
base_commit: b78f2bc159d1dc37a06df241285e21cadb675469
worktree_path: /home/herman/文档/notion-program
coordinator: codex

用户批准结构设计，要求减少维护者必须记住的同步修改点。设计见 outputs/state-incident-20261112/STRUCTURE-DESIGN.md。
主代理负责运行时所有权、MCP接入、配置统一、集成、版本、提交、发布及真实桌面读回。
Raman 在 /home/herman/.codex/worktrees/runtime-install-worker/notion-program 的 codex/runtime-install-worker 分支修改安装脚本及隔离Python测试。
Beauvoir 在 /home/herman/.codex/worktrees/runtime-workers-worker/notion-program 的 codex/runtime-workers-worker 分支修改任务组、观察器及App关闭语义。
两个工作树基线均为上述提交，各自暂存成果，不改共享context、版本或提交。
安装器不再接受业务配置或启动独立serve。MCP同一正式实例共用插件持有的运行时，配置只在初始化时加载。
运行时释放需停止并等待所有扫描、连接、归档及监听任务；持有者退出后其他入口可重新取得所有权。
保留原生产数据目录、12条备注和真实归档位置；自动归档暂缓设置不自动恢复。不以标题推断或批量改列恢复。
保留31条冻结需求、参考模型和锁，新增测试绑定到既有需求。独立候选自动验证不冒称完整release_ready。
实施过程和实际部署分别记录，未经真实目标环境读回不声称已经切换。跨平台发布与遗留远端问题分别报告。
