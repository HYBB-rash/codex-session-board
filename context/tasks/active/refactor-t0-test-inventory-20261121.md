# T0 · 测试盘点对齐主线

- owner: T0 作者
- execution_mode: isolated_write
- coordinator: Herman 的第一轮重构主会话
- base_commit: `b859880165908a848a4982d2cfd9b722e9aeb912`
- worktree_path: `/home/herman/.codex/worktrees/794e/notion-program`
- allowed_write_set: 本卡、同名 `context/results/` 记录，以及委派指定的源码外工作簿、差异和核查材料。
- 分支 `codex/refactor-t0-test-inventory` 已在干净工作树从 `origin/main` 建立并 rebase。领取时 `git fetch origin main` 的 `FETCH_HEAD`、远端跟踪引用和工作树提交相同。

维护者要改一条测试时，先按完整 ID 找到保护对象、行为、依据和唯一改动归属，再沿承接 ID 核对原断言。T0 维护清单，认领范围是旧表 1188 个 ID 与现役绑定 ID 的并集。逐个 ID 见源码外差异。

首件是 `测试ID差异首件-565bfaa-b859880.tsv`。先对齐原工作簿、旧绑定与领取时绑定，记录新增和退出原因，再用本机实际收集纠正平台口径。主要疑点包括同名测试改义、已删 PR 文档的旧引用、跨入口候选的独有断言和 macOS 证据范围。

A 负责保存回执，C1 负责远程归档数据归属。需整项裁决与重复合并的候选分别留给第二轮 T1、T2；业务路径相交时以已认领作者为准。协调者负责登记、归档和合入顺序。T0 不改业务、测试、绑定、参考模型或锁。
