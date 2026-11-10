# 默认工作区无项目新建落地方案与实验

owner: codex
execution_mode: shared_readonly
allowed_write_set: context/tasks/active/standalone-new-session-plan-20261111.md, context/tasks/completed/standalone-new-session-plan-20261111.md, context/STATE.yaml, context/journal/2026-11-11-standalone-new-session-plan.md, Docs/STANDALONE-NEW-SESSION-PLAN.md, Cargo.toml, Cargo.lock, outputs/standalone-new-session-plan-20261111/
base_commit: 4704f93c53d0bec35fc0f7306c02c53e0a0fc1a3
worktree_path: /home/herman/文档/notion-program
coordinator: codex

用户要求先形成落地方案，授权为消除影响计划的不确定性开展实验。
目标为本机默认工作区明确无项目新建。业务实现与部署不在本轮交付范围。
只读代理检查客户端独立会话命令和看板通信接口；协调者操作实验和维护共享记录。
允许验证新建输入页，不自动发送模型输入，不覆盖已有草稿或修改生产偏好文件。
验收重点为显式无项目语义、工作目录、草稿和窗口影响、可供看板调用的入口。

调研与方案完成，详见 Docs/STANDALONE-NEW-SESSION-PLAN.md。
真实快捷键观察到无项目选择；实时窗口出现其他操作后停止 UI 实验。
当前安装包原函数六个隔离分支通过；内存中双点入口扩展九个断言通过，
完整修改 bundle 语法检查通过。没有修改安装包、业务代码或部署，
没有直接改写偏好文件；真实快捷键按客户端原有语义改变了项目选择。
推荐社区客户端受管独立深链及能力报告，转发原生 newProjectlessTask，
看板只在本机默认工作区发送明确 standalone 目标。
真实补丁安装、能力报告及首轮完整客户端验收留在后续实施阶段。

Clippy 和提交钩子的全部测试通过，但测试期间其他操作提交了 1a86c73，
Git 因 HEAD 并发变化拒绝本轮提交。该提交没有递增版本，
随后的提交前检查拒绝其历史版本，初次收尾保留暂存内容，没有绕过钩子。

用户随后明确授权备份原提交并修正版本，再单独提交本轮方案。
原提交保存在 refs/codex/backups/standalone-before-repair-20261111，
原暂存树保存在 refs/codex/backups/standalone-index-20261111。
在隔离副本中将原规则提交重做为 1fc107d、版本 0.2.80，
保留原 AGENTS.md 内容、作者和提交说明，仅补齐两个版本文件。
本轮方案另按 0.2.81 提交，两个提交均经过完整提交钩子检查。
整合时核对共享分支仍为原提交，保留另一项调查的工作区及暂存区改动。
