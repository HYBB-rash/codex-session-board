# 原生与远程扩展验收汇总，2026-11-09 已交付

owner: Cicero
execution_mode: isolated_worker
allowed_write_set: scripts/verify_remote_release.py, acceptance/test_remote_release.py, context/handoffs/remote-release-gate-worker.md, outputs/owned-release-guard/, outputs/legacy-external-runner/
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

过滤夹具已交付；真实接线修订后，协调者解包复核 64 项离线测试通过。
下一阶段制作可重复执行的 legacy 外部验收驱动，并用独有 CODEX_HOME、
临时工作目录、loopback 确定性模型端点和专有实际 daemon 试跑。
仅操作该隔离环境中的测试会话、进程和配置，不连接或改动默认 daemon、
真实 SSH、生产数据、用户凭据及客户端界面。不替代目标客户端 UI 证据。
驱动接受实际看板 binary 路径，不复用旧 binary 签收最终版本。

汇总器、受限协议夹具和真实CLI隔离驱动已集成；最终v0.2.38冻结31项与
原生/远程16项由协调者实跑、独立内容复核及原汇总器闭环。
实际最终报告为
`outputs/remote-parity-integration/release-assembly-0.2.38-final-v2/remote-release-final.json`，
退出0、37条记录全部通过、release_ready=true。此结论不扩大已记录的环境边界。
worker未修改共享状态，任务归档及登记由协调者完成。
