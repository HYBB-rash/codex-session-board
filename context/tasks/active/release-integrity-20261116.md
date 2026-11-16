# 发布证据与双平台原子渠道

owner: /root/release_pipeline_audit
execution_mode: isolated_write
allowed_write_set: scripts/release_plugin.py、scripts/publish_plugin.py、acceptance/test_saved_release.py、acceptance/test_delivery.py、本任务记录
base_commit: aa1f5663eb09add4aef2624b070935462d25342b
worktree_path: /Users/hongyong/.codex/worktrees/merge-release-publish-20261116
coordinator: /root
status: ready_for_integration

沿用 save_platform 和 publish_saved，启动报告保存为 STARTUP-<target>.json，
读回核对 GitHub 资产摘要和包身份，候选发布必须具备两平台成功且清理完成的报告。
stable 继续通过完整外部验收 authorize。

publish_plugin 保留 publish 单包接口，新增 publish_many(repository, packages, channel)，
packages 为 (package, evidence) 配对。CLI 的 --package 和 --evidence 接受重复参数；
发布协调者使用明确的 --packages 批量参数，避免旧 source 内的单包脚本静默忽略前一个包。
旧 source 不支持批量参数时直接拒绝，保留上一可用渠道。单包输出保持兼容，
多包统一准备后以一次 git push --atomic 更新渠道。
失败保留上一可用双平台渠道，不替换不可变包。

只执行定向测试和必要提交检查，不操作日用插件或客户端。

已完成：

- STARTUP 资产先校验再上传并读回；再次保存复用同一资产，候选 finalize 缺证拒绝。
- 严格核对源码、构建指纹、二进制、版本、平台、全部 CHECKS、成功和清理状态。
- stable 继续要求原有完整外部证据；旧 stable 缺 STARTUP 时仍依赖原完整验收。
- 两平台先全部 authorize 和 prepare，远端引用前置核对和显式 lease 防止覆盖并发更新，
  一次 git push --atomic 同步更新；单包 publish 继续可用。
- 本地 bare Git 远端 update hook 拒绝第二个平台时，两平台旧引用均保留；
  移除故障后原子更新成功，再次执行不增加提交、不更改包。

定向验证：

- 首次启动资产改造：14 项通过，0.218 秒。
- 启动证据、真实 Git 原子拒绝与重试、stable 证据保护：17 项通过，3.439 秒。
- 补充并发引用变化拒绝后同一组 17 项通过，3.255 秒。
- 这些是本地受控验证，不代表真实 GitHub 发布或流程节省测量。

协调者需统一补 acceptance/bindings.json 新测试绑定，并运行最终组合与真实发布流水线。
