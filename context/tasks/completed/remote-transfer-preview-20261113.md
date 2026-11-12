# 远程拖动等待过场预览

owner: codex
execution_mode: serial_write
allowed_write_set: context/tasks/active/remote-transfer-preview-20261113.md, context/tasks/completed/remote-transfer-preview-20261113.md, context/STATE.yaml, context/journal/2026-11-13-remote-transfer-preview.md, 版本检查要求的清单文件
base_commit: fb1d3d4c1a7ea23f73abc596769ca865cd5c8587
worktree_path: /Users/hongyong/Developer/codex-session-board
coordinator: 当前会话

用户希望先调研并预览几种远程拖动等待过场，再决定正式界面的设计。
本轮只读现有界面和等待流程，在会话专用预览目录制作可交互动画。
不修改正式界面、服务、插件、会话状态或真实主机配置。

先比较已有产品的等待反馈，再制作风格有区别的过场。
预览包含快速成功、普通远程等待、较慢响应和失败情景。
成功反馈由模拟回执触发，循环动作不表示实际完成百分比。
交付为设计预览，不签收新业务功能，不替代真实客户端验收。

## 结果

已制作四个可播放方案，共用看板上下文，支持轮换比较：

- 微光落位：目标列立即接住待确认卡片，卡片边缘流光，底部显示回执状态。
- 远程传送舱：居中小面板、两端主机和往返信号，明确表现远程等待。
- 小小搬运工：搬运卡片的角色原地小跑，成功时跳跃，超时则保留待核对状态。
- 一封回执：目标卡片待确认，底部回执显示已接住和远端确认两个节点。

独立预览保存在本次会话的 remote-transfer-concepts.html，未改 index.html。
微光与回执包含缩小交互锁定范围的设计建议，正式接入时需单独实现和验证。
传送舱与搬运工覆盖等待期间的看板内容，更贴近当前整页等待的交互。
现有 moveCard 使用全局 saving；openSession、拖拽、刷新和多处操作共同依赖该标志。
persist 等待 /api/state 返回后更新前端记录，本轮只检查该行为，没有更改状态写入策略。

## 参考

- Google 将点阵动效作为等待、倾听和思考等状态的表现：
  https://design.google/library/evolving-google-identity?preview=true
- Material Expressive 的形状变化等待指示器：
  https://github.com/material-components/material-components-android/blob/master/docs/components/LoadingIndicator.md
- Slack 加载语句把等待文案写得轻松；其团队也指出加载语句不能长期遮掩性能问题：
  https://slack.engineering/getting-to-slack-faster-with-incremental-boot/
- Mailchimp 的 Freddie 击掌在成功操作后给短暂庆祝反馈：
  https://mailchimp.com/resources/designing-the-freddie-high-five-animation-for-facebook-ads/
- GitHub Primer 区分未知等待和真实进度，建议局部变化尽量保留已有界面：
  https://primer-docs-preview.github.com/product/ui-patterns/loading/

## 验证边界

独立浏览器检查普通响应、无回执、再次核对、本地快速完成、慢响应提示，
以及 320、736、1024 像素宽度无横向溢出、深色主题与减少动态效果，共 10 组检查通过。
已审阅四个等待状态及深色减少动态效果的截图。
测试创建的浏览器进程组退出且临时 profile 已清理。
预览中的计时、回执、失败和核对全部为本地演示数据，不访问真实远程主机。
本轮没有新业务实现，完整真实客户端验收状态继续沿用已有待验证结论。
首次提交检查遇到已有 RPC 拒绝连接测试失败，单项复验通过；原失败日志保留。
