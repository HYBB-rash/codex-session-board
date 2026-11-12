# 隔离开发改动重放到统一主线

owner: codex
execution_mode: isolated-worker
allowed_write_set: 原隔离开发八提交涉及文件及本任务卡；不写共享 STATE、primary main、渠道或正式实例
base_commit: 22340a6
worktree_path: /home/herman/.codex/worktrees/mac-dev-environment/notion-program
coordinator: 01a1ca27-a788-7a51-bc4a-aae07f5598a4

用户授权两个会话配合，使 main 正确且日用 Codex 加载最新插件。
发布协调者独占主线集成、推送、候选发布和两端日用切换。
本 worker 将 8e34863 的八个改动重放到最新发布基线，按父提交顺延版本，
保留 singleton、SQLite 恢复、保存原包发布流程及其验收绑定。
保留原开发验证历史，不将旧指纹证据冒充本轮统一源码的完整验收。
检查、回归和交付材料保存在源码之外；最终交协调者登记及归档。
