# 远程空会话持久性与剩余签收

2026-11-08，协调者复核 Lorentz 的独立交付，并接入正式实现。
主工作树为 /home/herman/.codex/worktrees/remote-parity-lab/notion-program。
本记录不表示远程功能已完整验收或部署；生产仍为 0.1.87。

## 已确认的根因与实现

仅 thread/start(ephemeral=false) 返回的零输入对象可能没有 rollout。
关闭创建连接或重启 daemon 后可能不可读。真实旧命名 seed 与新未命名
请求的差异经隔离正反例解释，不能把预计 path 或本地账本当作落盘。

实际 standalone 0.167.0 与客户端内置 0.166.0-alpha.6，
各验证 default/legacy、命名/未命名及命名后不读不恢复的 cold-only 对照。
8 个命名样本立即生成 session_meta rollout、标题索引及 has_user_event=0
记录，关闭连接及正常重启后独立 read/resume 成功，保持零 turn。
4 个未命名对照冷恢复失败。94 次 RPC，零 full-read、零 turn/start、
零模型请求；118 项证据一致性检查通过，自建进程与目录已清理。
掉电、崩溃、Native 输入页及首轮模型续写不在该结论内。

正式路径显式选 legacy，创建回执先保存 ID，再在同一连接设置初始名称，
读回同 ID、名称、空执行记录后继续已有项目登记；失败保持 created。
重试只读原 ID，不重放创建。新增两项回归及 DraftWriter 权限检查通过。

## 真实 Mac 的后端结果

0.2.35 的 owned-only 夹具仅转发一次 thread/start、一次初始命名、
一次项目移动，所有写入均在白名单内。HTTP 成功，同请求的 MCP 返回同 ID。
创建与看板连接关闭后，Codex app read_thread 独立读到同一主机、cwd、
初始名称与零 turn。只捕获导航 URI，没有打开或签收真实输入页。

- 新专用 ID：01a1b88f-70fd-73c1-b10b-10f856dfd5a6。
- 主机：remote-ssh-codex-managed:f6cdae6b-9c38-4789-9071-db5e90f93a7c。
- 项目：413e40a6-c61e-44de-ba5a-85f930d82cba。
- 请求：f80c1603-18f9-4b9f-9654-e78f6f846be8。
- 旧未命名请求 042792dd-8e7a-4f2b-be6f-5cfb6111a73b 保留原账本；
  Native 与直接读取均找不到其返回 ID，没有因失败自动重复创建。

空对象尚未出现在看板卡片及完整历史列表；不能据此否定已证实的落盘。
尚未完成实际远程状态保存、HTTP/MCP/自动标题、归档保护的正向签收。
不要通过注入用户历史、修改数据库标志或发送隐式模型输入制造通过证据。

## 原生入口限制

已安装 Desktop 为 26.1029.61003；内置 CLI 与 standalone 版本不同。
codex://new 的主入口只查本地项目；SSH add URI 还会修改连接偏好，
且只按 alias 复用主机。当前真实目标 alias=null，不能拿 hostname
冒充 alias，否则可能重复创建主机。未调用这些 URI 作为远程新建流程。
create_thread 空 prompt 工具调用被参数校验拒绝，没有返回新任务 ID。

## 证据与恢复

完整 worker 交付及 1536 个文件哈希已经核对并保存在
outputs/remote-parity-integration/native-new-inspection-evidence.tar.gz，
SHA256 f8ec91764236a97a7ddb48d2ae6ff86f0dbbfd8909c4354e2d745471be4b172f。
仅排除构建缓存与 __pycache__，包含完整原 handoff、源码摘录和实际协议。
真实后端、白名单审计及 Native 工具读回见同目录
new-empty-backend-0.2.35/；完整 472 项门禁自动部分通过，release_ready=false。

桌面在 2026-11-08 08:12 仍锁定。后续先解锁，沿用上述新专用会话，
明确授权一条测试输入后验证真正输入页、活跃来源、标题、状态、归档及导航。
同时核对首次输入的自动命名、模型与权限设置及客户端工具能力，
不能仅凭空对象可恢复推断它们与原生新建完全相同。
默认 SSH Agent 路径仍须验收，临时测试认证不得放入生产配置。
随后完成同一最终源码/二进制的冻结完整门禁、远程专项和正式切换。
