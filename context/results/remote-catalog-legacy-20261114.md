# 目标主机旧状态投影与会话归属

日期：2026-11-15。分支：`codex/remote-catalog-legacy-20261114`。
协调者负责父模块、RPC、快照和 UI；本任务仅新增两个纯模块及其测试。

入口为 `legacy::Projection::parse(state, codex_home, account)`，
`membership::resolve(thread, backend_projects, projection, host)`。
投影支持 Clone、Serialize、Deserialize，仅保留目录与归属所需字段。
`project_key` 统一生成 `target:` 加 JSON `[host, origin, id]`。
`membership::legacy_key`、`membership::deleting` 可供目录合并复用。

投影先核验账户 userId、workspaceRouting.chatgptAccountId 的 JSON 数组 SHA256
与 electron-renderer-content-owner 相等，再读取该 owner 的标记和账户桶。
live atoms 按 atom 浅覆盖 owner atoms；迁移命名空间固定为目标 `local:<codex_home>`。
仅保留当前 local-projects 的旧映射；历史映射不能复活已经消失的目录。
投影包含 assignments、projectless、membership_hosts、workspace_hints、checkpoint、
pending_deletions、references、sections、deleted_section_ids 与 issues。

references 的 id/threadId 为规范 UUID，origin 为 pinned/custom，
qualified 为布尔值，ownership 为 local/unqualified，logicalSectionId 可为空。
任一明确 foreign 主机证据会排除引用；迁移、删除、待删除标记均按目标 local 处理。
sections 保留目标 backendId、migrated 与 legacy 标记，便于协调者过滤后台 replica。

归属保留 Missing、Unassigned、Assigned 的区别，并使用逐线程迁移组合。
明确 projectless 阻止路径推断；已完成迁移的 backend null 保持未分配。
未验证账户时保留后台有效归属，未分配项标记 legacyUnavailable，停止 cwd 推断。
后台与旧意图冲突保留有效后台项目，同时报告 backendLegacyConflict，交由协调者复核。
同路径不同项目、映射矛盾、跨主机与待删除项目均有明确 issue。
backend roots 使用实际 roots:[{path}]；旧项目使用 rootPaths，不访问本机文件系统或 Git。

独立 Rust harness 已通过 15 项测试，包含八种迁移组合、明确移出、worktree 手工归属、
错误账户、旧映射、metadata 关联、跨主机、同目录冲突及已删除 section。
clippy-driver 使用仓库 41 行函数限制和 deny warnings/unwrap/expect，通过。
外部 harness 位于 `/tmp/codex-remote-catalog-design-20261114-30171-Y7F3ty/`。
完整集成编译、RPC fixture、真实客户端验收由协调者执行；本任务不操作日用后台。
