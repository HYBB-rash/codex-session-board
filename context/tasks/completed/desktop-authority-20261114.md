# 验证 Mac 共享执行服务并修复状态来源

- owner: codex
- execution_mode: serial_write
- allowed_write_set: 本任务 context；共享服务隔离验证、统一连接层、相关调用与自动门禁、测试绑定及文档；不修改版本清单或日用客户端配置
- base_commit: 54096cd
- worktree_path: /Users/hongyong/Developer/codex-session-board
- coordinator: 当前会话主代理
- status: completed_development_and_isolated_validation

用户要求先验证 Mac 能否共享桌面客户端实际使用的后台。通过后继续实现统一连接层和自动门禁，修复运行中的新任务被误判为阻塞。

当前日用客户端通过 stdio 使用独立 App Server，看板配置却指向另一个守护进程。后者没有加载当前会话，会将未结束轮次投影为 interrupted。此前直接接入私有 App Tools 管道及从外部启动内置 MCP 均被签名校验拒绝，不重试绕过。

首阶段使用隔离数据、真实 Mac CLI 和桌面传输实现验证同一执行服务的共享与生命周期。传输组件试验不冒充当前日用客户端接入成功。只有确认宿主接入可用后才继续业务连接层；失败保留原因和续作条件。日用安装、客户端重载和正式配置切换不在本阶段执行。

后续连接层必须区分实时服务与历史读取，连接归属和重连由程序自动校验，失联保留卡片状态。门禁覆盖同一历史目录下的错误守护进程、客户端重启、陈旧端点和多客户端，保留 31 条需求、参考模型及锁。

Mac 首阶段已通过：原装 ChatGPT.app 的独立 profile 在私有 CODEX_HOME 中，通过 CODEX_APP_SERVER_WS_URL 的 ws+unix 地址连接同一个 0.169.0 后台。桌面日志确认 websocket 初始化成功且没有 stdio 子后台；外部 Unix 连接的内核 peer PID 与后台 PID 5686 一致。测试客户端 5707 和后台均已停止。证据 /private/tmp/csb-host-gazc892a/report.json。

失败证据保留：只设置 CODEX_APP_SERVER_USE_LOCAL_DAEMON 时，当前 Mac 应用因注入配置覆盖而退回 stdio（/private/tmp/csb-host-zdfg4urf）；只隔离环境而未传 --user-data-dir 时被现有单实例接管（/private/tmp/csb-host-z6_6g4n3）。后续入口必须使用显式 Unix WebSocket，并同时隔离测试 profile，不能依赖该 daemon 开关。

实现入口 scripts/shared_desktop.py 的真实隔离复验已通过，使用客户端自带的 0.167.0-alpha.4.1，GUI 53650 和外部连接共同使用后台 53643，退出清理已确认；证据 /private/tmp/csb-shared-desktop-final.json。首个本地检查点 d8fde27 包含父后台进程归属、复用约束、notLoaded 状态保护及四项回归，未升版。

统一入口和四项启动回归、两项 MCP 跨进程状态及隔离回归均通过。随后清除 probe 继承的日用环境再做真实 Mac 复验通过（GUI 2894 / 后台 2890；/private/tmp/csb-shared-desktop-sanitized.json）；原装后台的 initialize、thread/list、thread/turns/list 和 mcpServerStatus/list 接口已独立核对（/private/tmp/csb-shared-protocol-methods.json）。临时隔离开发环境启动、归属检查及停止通过，cleanup_confirmed=true，报告 /private/tmp/csb-dev-authority-fpz_52_0/proof.json。

首次完整候选检查 /private/tmp/csb-authority-candidate-20261114 在 Clippy 阶段拒绝新测试中的四处 unwrap，保留失败记录并改用显式错误处理；业务断言、阈值、需求和参考模型未放宽。待重新执行完整候选检查。

第二轮 /private/tmp/csb-authority-candidate-20261114-r2 完成 685 项，684 通过，旧 section_config 回归要求先执行未经登记的代理，与新的强制归属规则冲突。保留失败并改为断言明确拒绝原因、代理未执行及数据未创建，未降低迁移保护。复查补充真实终态在卸载后仍可生效，以及临时继承 socket 不进入持久配置指纹，新增外部 stop 闭环。源需求与参考模型锁不变。

## 完成结果

最终代码检查点 ad2d392226e1d571f97cd0fdd7f65d09cfcea6f3，源码指纹 74b4bd9376ad801a9f6b35b83c7e1170c7cbd3ecd4cff62b45c4d147abd65773。第三轮固定候选 /private/tmp/csb-authority-candidate-20261114-r3 的项目检查、全部目标 Clippy 和完整自动验收均退出 0；687 项全部通过，零失败、错误或跳过。31 条仍为 17 通过、14 项真实环境待验，release_ready=false，不宣称完整验收或日用部署完成。

本机连接继承共享 Unix WebSocket，逐连接与请求核对 MCP 父后台的 PID 和启动时间，不依据同目录或 daemon version 选择来源；复用也检查后台归属。独立历史读取保持隔离，未结束的 notLoaded/interrupted 不参与阻塞推断。真实完成、带结束证据的卸载终态、人工状态保护及外部停止均有回归。业务绕过连接层的普通直接调用由编译门禁拒绝，未登记代理在执行前被拒绝。

稳定证据目录：/Users/hongyong/.codex/visualizations/2026/11/14/01a1d89d-57e0-71a2-9058-0775f92bc693/backend-verification。包括最终 validation.json、automated.json、真实 Mac 共享报告、GUI 日志、接口响应和隔离开发闭环；前两轮失败也保留。

隔离开发首轮验证器误用 runtime.start 的临时 root，该入口内部仍选择标准任务目录，被既有可执行文件身份保护拒绝；该目录保持 stopped。改为直接在临时 root 运行受控 supervisor 后通过，测试环境已停止。所有真实 Mac probe 均停止自己的客户端及后台，未重启日用客户端。

交付范围是本地源码及开发验证；版本清单、共享 STATE、日用配置和正式插件未改动，没有推送或部署。首次日用切换需独立部署任务，将已审阅入口用于目标 home/profile，并完成该环境的功能验收。后续业务 Agent 使用 App::connection，无须自行选择或证明后台来源。
