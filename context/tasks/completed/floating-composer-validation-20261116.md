# 原生悬浮输入框验证

owner: codex
execution_mode: serial_write
allowed_write_set: 本任务记录、context/STATE.yaml、context/journal/、源码外隔离验证目录和专用测试会话
base_commit: 3328fbc
worktree_path: /home/herman/文档/notion-program
coordinator: 当前会话
status: completed

验证不修改官方客户端时，看板下方悬浮输入框是否可切换到指定远程项目的
新草稿，保留用户在这里输入第一句话；区分原生手动操作和插件接口能力。
同时核对原生输入框的尺寸、位置、展开、聚焦可控范围。
只读代理分别核对接口和布局，真实界面由协调者串行验证。
仅使用原程序及隔离配置，不注入私有渲染器、不改客户端或日用配置。
验证材料保存在源码外 codex-floating-composer-validation-20261116 目录。

原程序配隔离 MCP App 现场验证，未改客户端或日用配置：
ui/message 的 target=new、send=false 加 hostId/projectId 返回 -32602 参数无效；
active/false 成功预填并聚焦当前悬浮框，页面保留；new/false 成功打开默认
首页草稿，目标为“此计算机 / 选择项目”，并未指定远程项目。
提供目标 model context 获得 updateId，但未改变执行目标。
size-changed 900×600 和 fullscreen 请求作用于插件画布，未改变原生输入框几何。
所有探测没有发送消息。实际回执保存为源码外 observations.jsonl。

当前普通插件接口没有原生 composer 目标、宽高、坐标、停靠或 CSS setter。
原装代码存在用户 resize 和拖动吸附：紧凑态高度随内容，展开可调整高度；
尺寸受边界约束，停靠列表受客户端布局分支影响，不是自由持久 x/y。
本次没有完成真实 resize/drag 签收，Linux 控制工具不支持 drag，
边缘键盘探测超时；手动几何能力仅按原装实现确认，详见源码外 REPORT.md。

建议保留原生输入时沿用 SSH 新输入页，接受离开看板；
若必须原地并控制样式，则由看板自有输入区域承担，创建链路接线另做。
第一次测试资源被缓存，重建独立配置后才取最终回执；失败不算通过。
测试进程已停止，认证/profile 副本清理；原 app.asar 和日用配置哈希相同，
日用客户端 PID 仍存活。没有业务代码、版本、部署或完整验收变更。
