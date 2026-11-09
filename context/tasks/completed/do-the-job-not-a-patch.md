# 把事情做正确，不要打补丁

owner: cursor
execution_mode: serial_write
allowed_write_set: AGENTS.md, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/
base_commit: ecc9d96
worktree_path: /home/herman/文档/notion-program
coordinator: cursor

原先那句把新情况收成“另写一个函数，不要调用旧函数再加条件”。
实验里代理读到了这句，仍按任务里更具体的一句去打补丁。
约定改成：先把要做的事情做正确。旧代码自己写错了就改正它。
不要在旧代码旁边补一块，让旧代码继续跑去完成另一件事。
