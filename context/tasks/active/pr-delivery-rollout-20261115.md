# PR 交付流程主线发布与四项新 PR 集成

owner: codex
execution_mode: coordinated_write
allowed_write_set: PR交付流程发布、PR8至11组合及必要冲突修正、发布版本、相关测试和文档、本任务上下文
base_commit: d4b40bb3aae98569a310f1e412defbb1f20dddbb
worktree_path: /Users/hongyong/.codex/worktrees/new-pr-batch-20261115/codex-session-board
coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
status: active

用户授权先将已完成的 PR 交付流程合入远端主线，再处理新 PR 并实测效果。
复用既有 integrate、promote、双平台候选和不可变包验证，不安装或重载日用插件。
三个独立只读审查分别覆盖验证底座、远程分区、原生新建及目录；协调者独占写入。

开始计时的已核实时间为 2026-11-15T03:41:03Z。
远端主线核实仍为 d4b40bb；流程实现头 92203c7。
固定原 PR 头：8=36fed3a、9=fbe2100、10=9fd007b、11=0a71260。
先发布流程，再固定新主线处理上述组合；审查、准备、失败和重试计入相应阶段，
记录收尾另计。已有旧 PR 检查不能冒充新交付流程执行，实测明确记录源码与检查器。
31 条需求、参考模型与锁保留，真实客户端验收保持独立。

流程候选 2fcf151 在原协调工作树冻结，干净后才创建本批隔离工作树，
在等待流程双平台门禁期间并行审查和整合，不改变已冻结候选。
PR8 与交付入口冲突已采用范围分流：PR 保留严格第一版专用报告和实际包顺序，
candidate、preflight、staged 使用 R1 第二版凭据与托管清理；边界写入 VALIDATION。
新增真实 CLI 回归覆盖一次初始化、固定来源、只读计划、参数隔离及失败报告读取。
向 PR8、PR9 原会话投递修复请求均被应用拒绝，返回 already has an active writer，
未视为已送达；协调者继续修正，保留此协调失败记录。
