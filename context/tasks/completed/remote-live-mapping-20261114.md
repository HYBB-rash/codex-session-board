# 验证远程分区的实时对应关系

owner: codex
execution_mode: isolated_write
allowed_write_set: Docs/REMOTE-LIVE-MAPPING-20261114.md、当前任务记录、源码外 /private/tmp/csb-live-mapping-20261114 只读实验回执
base_commit: 966795746becd982ec1bdf1d2e6df735c369601e
worktree_path: /Users/hongyong/Developer/codex-session-board-live-mapping-20261114
coordinator: codex
status: completed

用户要求先验证现有实时接口能否可靠提供逻辑分区与远端后台分区的对应关系，
再判断绕过磁盘确认等待的方案能否落地。保持 Codex 桌面端、业务实现、
自动分区开关和已有会话不变。本轮只读查询真实接口，不创建或移动会话。

核对接口真实响应、客户端序列化字段和映射来源，区分直接获得映射、
通过成员会话间接核对、空分区与首次创建映射的限制。
已有 5.2 秒问题的主因仍未确定，不把本轮结果当成自然复现。
开发调查仅更新本任务与调查报告，不修改版本和共享 STATE。

已执行五次真实只读接口查询，客户端处理耗时 14.2–911.3 ms。
五个逻辑分区均返回 ID、名称和成员，但不暴露 hostSectionIds；两个分区为空。
当前安装包已为 26.1111.21308，重新定位新资源，确认序列化仍只转出三个字段。
另交叉核对到一个远程成员使用 local 键，不能按前缀判断物理主机。
本轮无法支持“从现有实时工具直接取得完整映射”，原方案不能原样实施。
既有五个远程映射已落盘，下一轮可验证初始化校验后复用，
但后台目录校验、新主机、映射失效和实际移动确认尚未验证。

报告：Docs/REMOTE-LIVE-MAPPING-20261114.md。
原始证据：/private/tmp/csb-live-mapping-20261114/summary.json。
前后分区、映射和成员相同；实验的 SSH、临时 agent 和 socket 已清理。
没有改 Codex、插件安装、业务代码、自动分区开关或已有会话。
