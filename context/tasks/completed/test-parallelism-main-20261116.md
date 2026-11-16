# 测试并行改动合入主线

owner: side-conversation-01a1e3fb
execution_mode: isolated_write
allowed_write_set: f2eb297 的七个文件、候选版本清单及本任务上下文
base_commit: 0aeb0db7edab43db6f47d0a79be274c0673dbe32
worktree_path: /Users/hongyong/.codex/worktrees/test-parallelism-main-20261116
coordinator: side-conversation-01a1e3fb
status: completed

用户授权将已实测的两线程改动合入远端主线。
仅机械移植 f2eb297，不引入主会话尚未通过的验证工具候选。
本地 main 工作树有独有提交，不重置、不移动该工作树。
远端 v0.3.96 草稿已属于 7ccdc64，保留其产物和原始故障记录。
通过已有升版入口逐次记录 0.3.96 和 0.3.97；前者不上传或发布，最终冻结后者。
两平台完整候选及不可变包隔离启动通过后，使用现有推广入口更新远端 main。
不安装、重载日用插件，不把定向实测当作完整候选验证。

## 结果

仅机械移植 f2eb297 的七个文件；0.3.96 过渡提交为 3986e4f，最终候选为
b0847c56b6f771747c25485f6d62d5ea7d2d5b00 / 0.3.97。
没有带入 7ccdc64 的验证工具改动，没有修改或删除它的 Release 草稿。
CI 43861045024 首轮通过：macOS 971 项、Linux 982 项全部通过，
两平台不可变插件包读回与隔离启动通过，Candidate gate 成功。
既有 integrate CLI 已核验报告、源码、实际包及主线基线，正常快进远端 main 到 b0847c5，
并读回确认。入口计时 1562.59 秒，即 26 分 03 秒，不含前面的调查、移植、版本准备和收尾。

本轮 CI 库 / 原生两组均明确记录 threads=2：
macOS 为 127.36s / 49.07s，Linux 为 128.55s / 44.89s。
macOS 工具钩子自测仍耗 620.52s、Python 140.18s；Linux 为 412.20s / 60.18s。
这些是本轮绝对耗时，没有同机器同源码串行 CI 对照，不能宣称整个 PR 提速百分比。

完整回执及下载的实际包保存在：
/Users/hongyong/.codex/visualizations/2026/11/16/01a1e3fb-c015-7200-8dc9-1223b83cbb21/parallel-tests/main-integration/

本地主 main 工作树的 c8a2f8a 独有提交保留，没有重置或移动该工作树。
未安装、重载日用客户端；release_ready 仍为 false，完整目标环境验收独立。
收尾只修改 context 记录，按已有记录专用门禁同步，不重建插件包。
