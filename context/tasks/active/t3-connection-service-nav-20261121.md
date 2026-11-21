# 连接、服务与导航测试精简

- owner: rust-contracts
- execution_mode: isolated_write
- allowed_write_set: src/rpc.rs、src/rpc/framing_tests.rs、src/rpc/login_tests.rs、src/rpc/runtime.rs、src/config.rs、src/service.rs、src/service/migration/tests/aliases.rs、src/workers.rs、src/app/target_drafts/tests.rs、acceptance/test_native_new.py、acceptance/bindings.json、本任务卡及唯一结果记录
- base_commit: f15754129ac64a1dd02ec93abb00f9237bf0c9f8
- worktree_path: /home/herman/.codex/worktrees/t3-connection-service-nav/notion-program
- coordinator: board-refactor-five-rounds

核对 T0 的 17 项连接、配置与服务、导航断言，以及五项 RPC 与迁移合并候选。
以实际调用回执、外部接口、副作用与恢复结果为准，保留不同故障阶段和平台条件。
本任务只做独立本地提交和验证，交协调者集成。
