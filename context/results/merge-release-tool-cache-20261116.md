# 固定检查器缓存实现

owner: codex-tool-cache
execution_mode: isolated_write
allowed_write_set: scripts/check; scripts/checker/**; scripts/checker_cache.py; tests/git_hooks.rs; scripts/package_migration.py; acceptance/test_checker_cache.py; 本记录
base_commit: aa1f5663eb09add4aef2624b070935462d25342b
worktree_path: /Users/hongyong/.codex/worktrees/merge-release-tool-cache-20261116
coordinator: /root
status: active

固定检查器继续复用现有Rust源码、CLI和测试，不改变业务target归属或测试并发。
独立Cargo manifest只构建固定工具，当前候选的clippy、Rust测试构建仍执行根build.rs。
工具依赖锁、源码和构建条件固定后缓存不可变二进制，只锁构建过程。
