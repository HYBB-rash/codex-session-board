# 官方客户端远程新建验证

owner: codex
execution_mode: serial_write
allowed_write_set: 本任务记录、context/STATE.yaml、context/journal/、源码外专用验证目录、独立官方客户端测试配置、专用测试会话
base_commit: 9aea76b210fd8ae2f1d5c4cd1de79789db582fc8
worktree_path: /home/herman/文档/notion-program
coordinator: 当前会话
status: completed

用户授权在不修改官方 Codex 客户端的前提下，验证官方 SSH 入口与空输入
create_thread 两个方向。若完整需求不能满足，给出有实测依据的最小让步方案。
不得采用修改 app.asar、修改独立客户端副本、注入私有 renderer 代码或伪造来源的方案。
远端目标为 Mac 的任务看板项目，目录 /Users/hongyong/Developer/codex-session-board。
操作端项目 ID 84507417-6544-4224-a3e7-6871bfb0e446；
目标后台项目 ID 01a1d38a-3ab5-7741-a555-3aafd1ecb9a8，分别核对。

两名子代理只读核对工具校验和 SSH 路由，真实客户端操作由协调者串行执行。
保留日用客户端及任务，不安装或切换改版候选，不发布插件。
验证材料保存在 /home/herman/.local/share/codex-official-new-validation-20261116/。

初步结果：当前官方 create_thread 对空 prompt 返回 invalid arguments，无会话 ID。
同一官方工具接受明确的无工具固定标记输入，创建专用远程会话
01a1e419-7d61-7f92-bbd0-c59c5a47772b，首轮正常完成并返回约定标记。
官方 read_thread 已读回同一 Mac host、正确 cwd、单个已完成轮次。
远端独立 reader 确认 originator=Codex Desktop、cwd 正确；初始 projectId=null。
当前远端官方 0.169.0 支持 thread/metadata/update 的 projectId，
只对专用 UUID 补登记已有 canonical 项目 01a1d38a-3ab5-7741-a555-3aafd1ecb9a8。
停止写入连接、另起 reader 后仍读到正确项目，来源保持原生。

官方 SSH 深链实测打开 macbook / codex-session-board 的空白输入页。
通过正常输入框发送第二条专用标记测试，得到正确回复，真实 UUID 为
01a1e427-87bf-7963-8519-6ef7e1b045e6；远端来源原生、cwd 正确、一轮完成，
无模型工具调用。它登记了 alias 对应 host 和单目录 operator 项目，
但远端 projectId 仍为空，不能把目录匹配签收为 canonical 项目归属。

建议优先 SSH 入口：保留原生空白页与用户首条输入，让步精确内部连接/项目
ID 和创建前持久 UUID，把初版项目收敛为单目录工作区。
若精确 canonical 归属和即时真实 UUID 更重要，可让用户在看板先输入
首条真实需求，由原生工具创建后通过官方元数据接口补登记；
两步非原子、需复用 UUID 重试，并检测当前实验性接口能力。
原生工具调用上下文如何接入看板按钮仍待实施，不能宣称插件已经交付。

本次没有修改项目业务代码、客户端程序或后台代码，也未直接编辑数据库。
写入限于正常客户端和官方接口创建的专用会话及其项目元数据。
日用客户端 PID 保留；独立三项测试进程按归属停止，复制的认证/profile 清理。
app.asar 和日用看板配置前后哈希一致。
详细结果和阶段证据见源码外 REPORT.md；未发布、未代签全量验收。
