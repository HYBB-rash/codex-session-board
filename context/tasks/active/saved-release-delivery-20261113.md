# 保存交付包后发布渠道

owner: codex
execution_mode: isolated-worker
allowed_write_set: scripts/release*, scripts/package_migration.py, acceptance/test_saved_release.py, .github/workflows/plugin-candidate.yml, Docs/, context/tasks/active/saved-release-delivery-20261113.md
base_commit: c6e49e99b08bf542757c184683860241c46e5d14
worktree_path: /home/herman/.codex/worktrees/saved-release-delivery/notion-program
coordinator: 当前会话；等待 singleton-integration 完成后登记共享状态并提交

采用 GitHub Actions、Releases 和现有插件打包器。版本标签绑定源码，平台包先保存并校验，渠道只消费已保存的包。
本地与 CI 复用同一入口，遇到同名包重新读取验证，禁止覆盖。双平台齐全后才发布不可变预发布；稳定发布保留完整验收门槛。
先在隔离工作树实施和测试，等会话 01a1cfd8-e4ce-7ca2-b069-df266f7e4f4e 完成单实例集成后，再同步基线、分配版本并提交。
