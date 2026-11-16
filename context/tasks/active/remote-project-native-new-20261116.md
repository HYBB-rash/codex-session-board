# 在目标主机原生项目中新建会话

owner: codex
execution_mode: coordinated_parallel
allowed_write_set: src/, scripts/native_new/, scripts/check-native-new.mjs, scripts/check-remote-catalog.mjs, scripts/check-target-drafts.mjs, index.html, acceptance/bindings.json, Docs/REMOTE-NATIVE-NEW.md, 本任务文件
base_commit: d7c1717428e0fb56312afa74f82db942d7813d3a
worktree_path: /Users/hongyong/.codex/worktrees/remote-project-native-new/codex-session-board
coordinator: codex

用户授权按照已验证方案实现、验证并提交 PR，使用项目 pr-task 技能。
不修改 Codex 后台，不安装或重载日用插件和客户端。
作者分支为 codex/remote-project-native-new。
冻结检查器为 /Users/hongyong/.codex/worktrees/remote-project-pr-checker/codex-session-board，版本固定为上述基线。

目标后台项目直接使用真实 ID；旧项目仅在明确新建时按原生规则完整导入，
使用原旧 ID 作为幂等键，不迁移历史会话，不补写旧映射文件。
原生输入页以独立草稿身份保存主机、项目和目录，用户发送后由原生流程创建。
目标草稿不使用缺少项目身份的旧预热缓存，不写错误的旧归属。
同一操作重试保持身份；项目或来源变化、删除及丢回执如实处理。

客户端适配作者工作树：/Users/hongyong/.codex/worktrees/remote-project-native-client/codex-session-board。
项目准备作者工作树：/Users/hongyong/.codex/worktrees/remote-project-native-prepare/codex-session-board。
两者只提交各自范围；协调者审阅后组合，不并发改同一文件。
独立专用 PR 会话负责私有状态和巡检，不把原调查会话登记自动归档。

验证包括原生转换、RPC 边界、请求恢复、浏览器交互、当前客户端实际函数、
隔离真实后台与本地模型替身首轮，以及最终源码 PR 门禁和准确头 CI。
自动验证不代签日用客户端部署或完整 31 条目标环境验收。
