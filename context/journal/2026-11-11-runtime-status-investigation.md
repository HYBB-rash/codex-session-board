# 核查运行状态未自动移列

正式 v0.2.73 上确认两个真实反例，其中远程重命名调查会话实时 active，
原生分区与看板仍为待审阅。本诊断会话同样 active 但停在待判断。
两者都已持久保存目标 in_progress 的 pending，自动移列因客户端分区 socket
路径及两个发现目录不存在而失败，健康信息明确报 nativeStageError。
尚未查明路径为什么消失，未修改生产或重启进程。
4 项运行阶段测试和 9 项 socket 发现测试通过；这不代表现场已恢复。
证据与代码链见 context/tasks/completed/runtime-status-investigation-20261111.md。
