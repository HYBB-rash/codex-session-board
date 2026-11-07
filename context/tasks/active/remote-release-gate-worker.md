# 原生与远程扩展验收汇总

owner: Cicero
execution_mode: isolated_worker
allowed_write_set: scripts/verify_remote_release.py, acceptance/test_remote_release.py, context/handoffs/remote-release-gate-worker.md, outputs/owned-release-guard/
base_commit: 10c80203284d3f0fe069d3cb003113cdb26a97f6
worktree_path: /home/herman/.codex/worktrees/remote-connection-identity/notion-program
coordinator: codex

复用已完成连接工作者的独立工作树；原连接补丁保持不变。
协调者仍在主树整合连接代码，工作者仅写上述新文件，无交叉写集。
汇总器复用冻结验收器的指纹和附件校验，不能替代或降低冻结 31 条契约。
完整 legacy 验收与 native5/remote 专项证据必须绑定同一最终源码和 binary；
缺失、过期、错误模式、重复案例及任一侧未通过均拒绝完成。
不修改版本、测试绑定、共享状态或日志，不暂存或提交，不取证真实工作会话。

汇总门禁交付后，顺序制作独有 outputs/owned-release-guard/ 真实验收协议过滤夹具。
仅离线单元测试；本阶段不连接真实 daemon，不操作 UI 或真实数据。
所有请求、通知、线程创建回执均先做会话及主机白名单检查，再转发或记录。
