# 新情况单独写判断

owner: cursor
execution_mode: serial_write
allowed_write_set: AGENTS.md, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/
base_commit: 6e86d7f
worktree_path: /home/herman/文档/notion-program
coordinator: cursor

日常模式把“卡片动不动”接在旧函数调用旁边，一句一句加条件。
这是新情况借旧函数来用。旧函数自己的错误仍应直接改旧函数。
脚本分不清这两种改动，规则写在 AGENTS.md。
