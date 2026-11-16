# 远程原生新建

看板使用官方客户端已有的 SSH 链接，不修改、复制或重新签名客户端。
选择主机及单个远程目录后打开 Codex 原生空白输入页。
用户第一次发送消息时，Codex 才创建持久会话；点击按钮不会取得空会话 UUID。
看板不发送 thread/start、turn/start，不伪造 clientInfo 或 originator。

## 入口与登记

链接为 codex://settings/connections/ssh/add，仅包含 name、projectPath、
enabled=true、activate=true。name 来自当前客户端登记的 SSH alias，
projectPath 来自所选项目唯一的绝对目录；不传内部主机编号和项目编号。
每次导航重新读取登记，拒绝缺失或重复 alias、多目录和无效路径。
旧配置中的 native_new_capability 已不再使用，无须配置能力清单。

没有 alias 的历史连接，需先通过 Codex 设置 → 连接登记 SSH 别名。
看板不修改 SSH 配置或原生登记表，不把 hostname 冒充 alias。
同一台机器的旧连接与新 SSH 连接可能有不同内部编号，不能承诺复用旧编号。
客户端关闭或无法识别链接时，不会改用后台创建接口。
导航回执只表示链接已交给客户端，不表示客户端已经完成切换。
发送第一句话前，应核对原生输入页显示的主机和目录。

## 项目归属

正确的执行目录与后台项目归属是两件事。SSH 原生新会话的 projectId 可能为空。
不能按“最近创建、目录相同”猜测用户刚创建的是哪个会话。
补登记须选择实际会话 UUID，并向同一远端调用 thread/metadata/update，
再读回同一 UUID 的 projectId 和 cwd。重试只重试补登记，不再创建会话。
此接口在已实测远端 0.169.0 中为实验性接口，旧后台拒绝时保留原会话。

## 原生悬浮输入框

当前官方插件接口没有隐藏、尺寸、位置或远程目标 setter。
widgetHideActions、header.enabled 和 fullscreen 控制的是插件区域，不隐藏原生输入框。
不得添加无效的 hideComposer 元数据、注入原生页面或改客户端实现。
独立浏览器/PWA 看板没有 Codex 原生悬浮框；嵌入 Codex 的看板仍受宿主控制。

## 验证边界

2026-11-16 已用原装 26.1111.21308 隔离窗口实测 SSH 原生输入页、
用户首轮、正确远程 cwd、Codex Desktop 来源，以及同 UUID 项目补登记读回。
现场材料见本任务记录引用的源码外 REPORT.md。
这不替代本次实现的自动回归、交付后目标环境验收或完整 31 条验收。
业务连接统一经过 App::connection；不从 daemon version 的 socketPath 推断后台。
