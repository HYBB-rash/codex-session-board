# 远程原生分区操作耗时调查

owner: codex
execution_mode: serial_write
allowed_write_set: context/tasks/active/remote-section-latency-20261113.md, context/tasks/completed/remote-section-latency-20261113.md, context/STATE.yaml, context/journal/2026-11-13-remote-section-latency.md, Docs/REMOTE-SECTION-LATENCY-20261113.md, 版本检查要求的清单文件
base_commit: c15946e647b222029ee5938ec2ebdde740833155
worktree_path: /Users/hongyong/Developer/codex-session-board
coordinator: 当前会话

用户要求进入 Codex 源码定位远程分区操作耗时，可在 Linux 主机设计实验。
先核对真实客户端构建和实现，再区分请求、主机连接、后端写入与映射读回的耗时。
实验只使用隔离环境或专用测试对象；保留现有会话、备注、分区和连接偏好。
不恢复远程分区自动同步，不部署或重载日用客户端。
原始数据保存在 outputs/remote-section-latency-20261113/，报告注明源码身份和实测边界。

## 结果

已读取 Fedora 当前安装的 Codex 26.1106.51428 JavaScript 资源并记录指纹。
真实 M1 移动确认 191–942 ms；另一次受控实验中原生移动 230 ms 已完成，
连续改名另一张无远程映射的专用分区，使目标映射到 5689 ms 才在状态文件中出现。
源码确认为共享原子状态的 500 ms 防抖保存，每次更新重新计时，没有最大等待时间。

隔离运行同包 Linux app-server，10 次原生移动中位 0.84 ms，
不支持“接口固定等待五秒”的猜测；不能将隔离耗时当成真实远程总耗时。
抽取实际保存类和防抖函数的独立实验亦复现连续更新推迟落盘。
历史 5.2 秒缺少逐阶段日志，本轮证明同类机制，不补造旧事件触发证据。

报告：Docs/REMOTE-SECTION-LATENCY-20261113.md。
三张专用空会话已归档；测试逻辑分区、远程实际分区和隔离缓存已清理并核对。
没有恢复远程分区同步，没有部署、重载客户端或修改普通会话。
本次仅调查，不代签新业务实现和完整目标环境验收。
