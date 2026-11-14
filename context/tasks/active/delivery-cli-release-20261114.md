# 三个交付工具主线发布与完整计时

owner: codex
execution_mode: serial_write
allowed_write_set: 发布版本清单、project-rules.json及本任务context；授权推送主线和部署日用插件
base_commit: c6b87934645ec41efda485163e4ca205f27914ed
worktree_path: /Users/hongyong/.codex/worktrees/four-pr-rehearsal/codex-session-board
coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
status: active

用户授权将三个交付CLI纳入主线，并完整实跑合并、双平台候选验证、主线提升、
不可变包安装、当前客户端生效核验和现场功能检查，记录包含故障及等待的实际耗时。
远端main已核对为e6f186a141579154a9a8f50f252bc57d9203b726，是当前工具提交的祖先；
保留小提交，通过候选门禁后快进主线，不重复回放已经合入的四个PR。
对比上轮91分14秒总流程时，披露本轮合并内容较小、已知修复已经存在的差别，
分别比较CI和部署阶段，不将全部差值归因于工具。
完整31条验收仍保留外部待验证，不降低断言、不使用旧证据签收新指纹。
原始时间及回执保存在源码之外的delivery-cli-release-20261114目录。

0.3.76候选65aac31297f0db8768fd604bf5d31c694d302f69已通过Actions 43547817526：
Linux681项、Mac670项全部自动通过，31条仍17通过14待验，release_ready=false。
受保护主线已通过promote快进到同一提交，正式main工作区也已同步。
不可变发布v0.3.76已安装，Mac包Git版本7692d186cd2e51b66f5b53c6c483c2761bf644bd。
真实SSH性能门禁通过：状态中位数99.92ms、p95 164.65ms、36次摘要343.84ms。
远端检查准备中的缺依赖、PKCS8格式兼容及有效登记路径问题均计入本轮，
修正只发生在源码外测试环境，与CI等待重叠，不改变业务代码或日用配置。

安装后的verify-plugin明确返回awaiting_desktop_reload：当前客户端12342仍运行旧版。
board_view返回ready=true也未使新包生效，不能将其当作加载新版本的证据。
当前计算机操作工具禁止操作com.openai.codex，未通过其他GUI通道绕过该限制；
Mac现有定向重连接口无可用入口，也未拿不拥有当前会话的旧daemon代签。
已请用户在插件页关闭再开启会话看板并打开看板，随后继续自动检测和现场冒烟。
任务尚未完成，完整总耗时和节省量尚不能签收，计时继续保存在源码外。
