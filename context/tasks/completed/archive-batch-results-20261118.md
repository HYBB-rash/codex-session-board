# 一键归档结果

owner: codex
execution_mode: isolated_write
allowed_write_set: src/, tests/, index.html, assets/, scripts/, acceptance/bindings.json, Docs/, context/tasks/
base_commit: 5deef00dd55f4dace3b46b92bbf85570cd0eb446
worktree_path: /home/herman/.codex/worktrees/archive-batch-results/notion-program
coordinator: codex

页面逐条提交。看板保存不可变名单与每项结果，重开后从「归档结果」查看。核对通过同一主机与账号读取原生归档列表。原生结果未知时记录待核对，重复读取不会发送归档请求。

隔离测试覆盖丢失原生回执、重开后卡片已缺席、切换账号、停止尚未提交项、新输入拒绝及原生写次数。浏览器夹具覆盖回执丢失后的结果入口和只读核对；MCP 桥接夹具覆盖四个批次入口。
