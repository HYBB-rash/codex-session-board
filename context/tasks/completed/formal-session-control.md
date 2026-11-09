# 穷尽核对原生会话控制状态

owner: cursor
execution_mode: serial_write
allowed_write_set: src/, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/
base_commit: 4ee42bace00e21b07580ad546b7db3d1da5dc668
worktree_path: /home/herman/文档/notion-program
coordinator: cursor

冻结的 acceptance/model.py 继续只覆盖单轮人工选择和执行阶段，不修改契约锁。
本次为日常 native_sections 路径单独建立会话控制状态机，穷尽可达状态，
并把每条边重放到真实 Store。远程新建阶段单独穷尽。
不把这份模型写成整个进程、网络和客户端的证明。

从空白未读会话得到 5922 个可达状态。39 个事件在每个状态上重放，
230958 条边与内存 Store 逐字段一致。远程草稿 6 个阶段、30 条边同样一致。
Store 在分区版本尚未缓存时仍会写入人工状态，App 层拒绝该写入，已单独钉住。

未闭合的范围是 SSH 身份、侧栏 socket 恢复、页面、多会话交错、扫描进度、列排序，
以及两次读回之间的 ABA。不记成整份插件证明，也不记成 release_ready。

相关提交：
642482c test: exhaust remote draft stages against the store
b3cec01 test: replay every session transition on the store
476370d test: round-trip each native session state through sqlite
553d3c9 test: enumerate every reachable native session state
e7ad9cf test: model native session control as a finite state machine
