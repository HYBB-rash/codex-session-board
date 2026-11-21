# T2 第二组 · 发布验证拒绝来源不符的候选

三个入口原先都为同一夹具传入一个开关，在 `candidate.main()` 返回后要求退出码为 1，并从 `VALIDATION.json` 读回 `passed=false`。合并后，每个具名子测试仍单独创建源码目录、输出目录与报告。维护者可在同一处核对三种输入和共同的拒绝条件，失败输出会指明具体场景。

三个完整测试 ID 均以 `acceptance.test_candidate.CandidateGateTests.` 开头，原先各自只绑定 GATE-001。承接 ID 是 `acceptance.test_candidate.CandidateGateTests.test_candidate_rejects_a_report_for_different_source`。

| 原方法 | 承接子测试 | 唯一改变的输入 |
| --- | --- | --- |
| `test_candidate_rejects_new_uncommitted_changes` | `uncommitted changes` | Git 工作区状态返回 `dirty`，前后源码身份、报告指纹相同。 |
| `test_candidate_requires_unchanged_commit_even_when_fingerprint_matches` | `only the commit changed` | 后一次读取的提交号变化，树和源码指纹均相同，工作区干净。 |
| `test_candidate_rejects_a_report_for_different_source` | `report fingerprint mismatch` | 自动报告的 `build_sha256` 与候选指纹不同，提交、树和工作区均保持原值。 |

前两个入口及其绑定已删除。第三个入口保留 GATE-001 绑定，三个子测试沿用原来的调用及退出码、落盘报告两项断言。

Linux 基线逐个运行原入口，三项均通过。承接者另做了六次隔离对照，每次仅改变内存中的测试夹具或测试读到的报告副本。

- 分别把一个来源条件恢复为正常输入。候选入口随即返回 0，该子测试在退出码断言得到 `0 != 1`，另外两项通过。
- 分别让一个子测试在真实拒绝返回后读到 `passed=true`。退出码断言通过，报告断言失败，另外两项通过。

六次对照中验收记录器都把失败归到承接 ID。原始夹具下三个子测试均通过。逐案结果、实验脚本、最终提交号和全量收集清单保存在本机源码外的 `~/.local/share/codex-session-board-investigations/t2-candidate-source/`。
