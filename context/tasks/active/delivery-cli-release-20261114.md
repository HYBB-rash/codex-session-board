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
