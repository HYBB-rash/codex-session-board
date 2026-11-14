# 验证 Mac 共享执行服务并修复状态来源

- owner: codex
- execution_mode: serial_write
- allowed_write_set: 本任务 context；共享服务隔离验证、统一连接层、相关调用与自动门禁、测试绑定及文档；不修改版本清单或日用客户端配置
- base_commit: 54096cd
- worktree_path: /Users/hongyong/Developer/codex-session-board
- coordinator: 当前会话主代理
- status: implementing_verified_gateway

用户要求先验证 Mac 能否共享桌面客户端实际使用的后台。通过后继续实现统一连接层和自动门禁，修复运行中的新任务被误判为阻塞。

当前日用客户端通过 stdio 使用独立 App Server，看板配置却指向另一个守护进程。后者没有加载当前会话，会将未结束轮次投影为 interrupted。此前直接接入私有 App Tools 管道及从外部启动内置 MCP 均被签名校验拒绝，不重试绕过。

首阶段使用隔离数据、真实 Mac CLI 和桌面传输实现验证同一执行服务的共享与生命周期。传输组件试验不冒充当前日用客户端接入成功。只有确认宿主接入可用后才继续业务连接层；失败保留原因和续作条件。日用安装、客户端重载和正式配置切换不在本阶段执行。

后续连接层必须区分实时服务与历史读取，连接归属和重连由程序自动校验，失联保留卡片状态。门禁覆盖同一历史目录下的错误守护进程、客户端重启、陈旧端点和多客户端，保留 31 条需求、参考模型及锁。

Mac 首阶段已通过：原装 ChatGPT.app 的独立 profile 在私有 CODEX_HOME 中，通过 CODEX_APP_SERVER_WS_URL 的 ws+unix 地址连接同一个 0.169.0 后台。桌面日志确认 websocket 初始化成功且没有 stdio 子后台；外部 Unix 连接的内核 peer PID 与后台 PID 5686 一致。测试客户端 5707 和后台均已停止。证据 /private/tmp/csb-host-gazc892a/report.json。

失败证据保留：只设置 CODEX_APP_SERVER_USE_LOCAL_DAEMON 时，当前 Mac 应用因注入配置覆盖而退回 stdio（/private/tmp/csb-host-zdfg4urf）；只隔离环境而未传 --user-data-dir 时被现有单实例接管（/private/tmp/csb-host-z6_6g4n3）。后续入口必须使用显式 Unix WebSocket，并同时隔离测试 profile，不能依赖该 daemon 开关。
