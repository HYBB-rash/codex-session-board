# 发布证据与双平台原子渠道

owner: /root/release_pipeline_audit
execution_mode: isolated_write
allowed_write_set: scripts/release_plugin.py、scripts/publish_plugin.py、acceptance/test_saved_release.py、acceptance/test_delivery.py、本任务记录
base_commit: aa1f5663eb09add4aef2624b070935462d25342b
worktree_path: /Users/hongyong/.codex/worktrees/merge-release-publish-20261116
coordinator: /root
status: active

沿用 save_platform 和 publish_saved，启动报告保存为 STARTUP-<target>.json，
读回核对 GitHub 资产摘要和包身份，候选发布必须具备两平台成功且清理完成的报告。
stable 继续通过完整外部验收 authorize。

publish_plugin 保留 publish 单包接口，新增 publish_many(repository, packages, channel)，
packages 为 (package, evidence) 配对。CLI 的 --package 和 --evidence 接受重复参数；
单包输出保持兼容，多包统一准备后以一次 git push --atomic 更新渠道。
失败保留上一可用双平台渠道，不替换不可变包。

只执行定向测试和必要提交检查，不操作日用插件或客户端。
