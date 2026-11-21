# 会话看板五轮重构

owner: Codex 总协调会话 01a1fc2d-af6f-7d50-9759-33835b51eaeb
execution_mode: isolated_write
allowed_write_set: context/tasks/active/board-refactor-five-rounds-20261121.md、对应的交接/结果/完成卡、协调者任务登记，以及本次会话的交付物
base_commit: b859880165908a848a4982d2cfd9b722e9aeb912
worktree_path: /home/herman/.codex/worktrees/board-refactor-coordination/notion-program
coordinator: 本会话跟进作者与独立审阅，Herman 决定业务取舍和合入顺序

业务规则有一个决定结果的地方。每项保留测试说明受益者、实际触发条件及预期行为。各作者拥有独立任务卡和测试 ID，按最新远端主线重基，独立审阅和验证准确提交。主线前进后复核受影响的实现、构建及测试。

| 工作包 | 作者会话或领取条件 | 首件 |
|---|---|---|
| A 保存卡片 | Mac · 01a1fc5f-ab6b-7892-b2cf-ecef3dcdecc5 | 两个空格的请求、服务端回执、刷新前和读回后的卡片 |
| C1 远程归档 | Mac · 01a1fc5f-f25f-7013-9def-d2a2492e17b0 | 两处库的实际记录、丢回执恢复前后的原生写入次数 |
| T0 测试基线 | Linux · 01a1fc5f-040e-7a21-b4ce-6d94b255128b | 旧基线 1,188 项到当前提交的 ID 差异、退出原因、可认领清单 |
| B1 卡片归列 | Linux · 01a1fc60-c3f0-7a01-800f-97acba496bd5 | 原生、本地、远程事实与四、五、六列结果 |
| E1 回复预览 | Mac · 01a1fc61-077c-7320-8fc8-e48cb0d1758d | 网页与离线嵌入资源共用 Vue/TypeScript 预览，验证迟到请求与焦点 |
| T1 整项移除、T2 首批 | T0 核实对应候选后分别领取 | 三个实验反例的性质；旧标题回执对照与故障注入 |
| B2、D、C2、F | B2 接 B1；C2 接 C1；D 与 F 可穿插 | 操作资格表；方法/路径映射；远程字段归属；退役诊断及 README 分开交 |
| E2a、E2b、E2c | 接 A、B1、B2、E1 | 每次交付一个可运行的展示、编辑或拖动动作，同次删除旧路径 |
| E3、筛选、排序、收尾 | E3 接 E2、C1；筛选与排序分开交 | 十卡中断恢复、筛选后归档范围、键盘排序及组件库取舍 |
| T2 余项、T3、测试结清 | T2 按已核实清单认领；T3 由对应业务作者负责 | 每个候选有最终处置、出处、承接测试；最终主线复核平台、31 条需求与锁 |

第一轮首件均以本任务隔离环境取证，网页、打包嵌入资源和真实客户端分别记验证范围。原报告涉及的两份 PR 规划脚本已退役。

结项依据是逐包交付和远端读回；条件项可凭调查结论关闭。目标环境缺证须记录，完整验收以 `python3 scripts/verify_acceptance.py` 的退出码、`release_ready` 和经审阅的证据判定。
