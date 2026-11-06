# 状态分区与会话同步

owner: codex
execution_mode: serial_write
allowed_write_set: src/, tests/, scripts/, acceptance/, Docs/, README.md, index.html, config.example.json, context/, 本次授权的分区和部署配置与服务
base_commit: 836f734d095d7a308ef18959dbc7ebb1884e3eb0
worktree_path: /home/herman/文档/notion-program
coordinator: codex

用户要求撤掉已有自定义分区，改成看板的六状态分区，后续状态改变时移动原会话。
允许本次迁移原生侧边栏分区与会话位置，并部署所需同步行为。
保留会话和项目本身，不发送消息，不启动任务，不执行真实归档。
现有契约禁止移动分区，需明确记录用户本次要求对该边界的变更。

先检查真实原生 API，保存原始布局，再迁移和独立读回；代码验证使用本轮新证据。
只读代理分别调查同步设计和原生分区 API，均不得修改仓库或外部状态。

已实现并部署当前源码版本 0.1.18，未升版、未提交、未推送。
默认关闭的分区扩展保留冻结 31 条契约、参考模型和锁，新增边界说明及回归。
原生连接只允许读取布局和移动会话，标题开关与分区开关独立。
保存立即尝试，后台对账和独立读回，重启自然补偿；缺失分区、别名及连接异常可见。
桌面完整扫描确定移动范围，写前核对资格；轮换候选防止持续超时饿死后项。
实测确认已加载会话使用 client-new-thread 别名，新增可配置的只读身份映射。
客户端 socket 更换后在同目录中只接受唯一后继并重新检查权限。

迁移前布局、配置、SQLite 一致性备份和旧部署在 outputs/status-sidebar-20261106/。
创建六个状态分区，原生后台迁移本机 115 个未归档会话，撤掉七个旧自定义分区。
项目和远端固定会话保留，未发消息、启动任务或真实归档。
状态顺序及数量独立读回为待判断 98、进行中 1、待审阅 1、已阻塞 1、
待归档 14、已完成 0。
当前会话通过真实 HTTP 人工选择待审阅，验证原生移动后恢复原自动状态；
两次原生位置与 115 条最终状态均独立核对成功，证据 live-verification.json。
安装缓存与构建 SHA256 一致，已 skills/list forceReload 和 config/mcpServer/reload。
原生窗口观察未完成六分区视觉取证，不将独立 API 读回当作全量 UI 验收。

严格 Clippy、单元回归通过；完整验收 243 项自动通过，退出 2，
release_ready=false，其他真实环境证据继续待补充。
最终 ./scripts/check 与上下文校验结果记录在本任务对应日志。
