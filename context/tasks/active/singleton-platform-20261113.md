# Singleton 跨平台验收绑定修复

owner: singleton-integration
execution_mode: isolated_write
allowed_write_set: acceptance/test_plugin_update.py, scripts/verify_acceptance.py, scripts/bind_rust_tests.py, acceptance/test_gate.py, acceptance/bindings.json, tests/singleton_platform.rs, Docs/SINGLETON-GUARD.md, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/tasks/active/singleton-platform-20261113.md
base_commit: 22340a6
worktree_path: /home/herman/.codex/worktrees/singleton-integration/notion-program
coordinator: codex-mac-migration

主线协调者委托修复 macOS 缺失 Linux singleton 测试导致的库存不匹配。
保留 Linux 12 项测试与31条需求及其锁；Mac 验证配置边界与默认导航。
NAV-001 使用共同与平台分支，适配器仍要求真实测试库存完全相等。
不改主线、生产配置或运行实例，最终版本与候选发布由协调者串行处理。

验证：Linux 两项与 macOS 三项 singleton_platform 测试均通过，零忽略；
十二项 gate 单测通过，绑定生成器重复运行产物一致，提交前项目检查通过。
双平台完整验收报告保存在源码外；最终固定版本验收由协调者执行并审阅。
第九项 saved-release 测试随本提交补绑；尚未进入基线的第十项由协调者
随后续 Release 修复代码一并绑定，避免引用不存在的测试。

Mac 完整门禁发现默认临时根目录导致真实 Unix socket 路径超长；
PluginUpdateTests 使用短临时根目录，保留真实 socket 与全部权限断言。
