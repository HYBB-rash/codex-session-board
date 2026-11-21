# 远程实验测试承接

owner: Fedora 远程测试作者
execution_mode: 独立工作树
allowed_write_set: 两组 remote_parity_lab 用例、承接的 Rust 测试与夹具、acceptance/bindings.json、本任务卡
base_commit: 5f5d5d26addf432db0a1d834e0750d241052c032
worktree_path: /home/herman/.codex/worktrees/fedora-remote-parity-closeout/clean-integration-main
coordinator: 五轮重构协调者

归档六项与人工保存三项分别核对真实入口。需求依据见 `Docs/REMOTE-PARITY-LAB.md` 的风险表和 `Docs/NATIVE-STATE.md` 的并发保证。归档重启核对沿用现有远程批次测试。主机隔离使用同 UUID 的远端与本机 HTTP 保存，远端阻塞期间本机须已提交备注。
