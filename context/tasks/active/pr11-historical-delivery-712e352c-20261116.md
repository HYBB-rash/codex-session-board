# PR 11 历史交付修复

- owner: root
- execution_mode: isolated_write
- allowed_write_set: scripts/check-host-filter.mjs, acceptance/bindings.json, src/app.rs, src/rpc.rs, src/rpc/authority/tests.rs, src/app/remote_snapshot.rs, src/app/remote_snapshot_tests.rs, src/native_sections/direct.rs, tests/native_authority/archive.rs, tests/native_direct.rs, context/tasks/active/pr11-historical-delivery-712e352c-20261116.md
- base_commit: 0a71260f2c1e8e6ecfa06d0f9689e606318f95a3
- worktree_path: /Users/hongyong/.codex/worktrees/pr-queue-author11-20261115/codex-session-board
- coordinator: historical-four-pr-queue
- status: validation_pending

用户授权的历史合并演练。只在指定修复树工作，不合入当前主线或其他完整组合，
不修改版本、共享 STATE、需求、参考模型和锁，不操作日用客户端。
比较 base 为 22b9dc4e45c872d91e921c2f636d256001241ac6；
固定 checker 为 ec4e9c6bd73f477a6baf5d1ff6bceea6ed0a5e3b。

最初的 delivery 失败为缺少源码外 review-pending.json，已保留读文件失败的复现日志。
协调者分派的独立审阅通过原头，作者没有自签审阅。
原头首轮固定检查器耗时 1000.970 秒，769/770 项通过；
主机筛选回归期望只读故障提示，却读到旧项目归属提示。
UI 三项独立复跑通过，不能替代首轮失败。

受控浏览器复现确认：启动两个 load，延迟较新响应，较早 load 因 requestSerial
过期返回时仍是旧快照；较新响应完成后提示才正确。
测试刷新辅助函数为 fixture 快照加版本标记，等待该版本生效后执行全部原断言。
故障读取仍直接调用 load 并检查连接错误；断言、用例数和业务实现不变。

源码外输出目录：
/Users/hongyong/.codex/visualizations/2026/11/13/01a1d38b-1ef8-7392-ba78-caa7935c5ab4/pr-queue-20261115/author-11

首次失败保留在 validation-0a71260f2c1e8e6ecfa06d0f9689e606318f95a3-1；
竞态复现在 ui-snapshot-race-reproduction.log。提交后更新 ready.json，
由协调者再次分派绑定新头的独立审阅，使用新的源码外目录重跑固定门禁。
PR 9 所有权借用和 PR 10 新建请求的组合风险仍由精确组合候选验证。

## 第二轮冲突反馈 8ffd574d

固定原头 e9e4760841cc2fc363e87f425879c3c29a275e0a，
仅合入授权的组合前置 b27e164b6b034e270aab2fcde319246c99230ef9。
四处实际文本冲突保留在源码外 merge-8ffd574d-first.log。
该前置包含 PR10 和交付工具；不合入当前 main、PR9 或其他完整候选。
前置自带的版本清单、规则和共享 STATE 保持前置原值，不另做升版或共享记录修改。

RPC 同时保留限权 CatalogReader 和 PR10 的只读 DraftReader，删除旧 DraftWriter；
兼容模式继续禁止草稿读取与归档，错误后台拒绝测试覆盖全部七个角色。
测试绑定取两侧并集，保留目录、草稿及新增原生新建回归，不改需求或锁。
自动合并后的 createSession 已同时保留目录项目新建门禁及 PR10 的请求契约：
首次原生新建不带 requestId，只有已有草稿恢复携带旧 requestId。
主机筛选测试也保留前轮快照同步和 PR10 新建/恢复断言。
提交前运行组合编译和相关回归；冻结新头后重新请求独立审阅及固定检查器验证。

## 补充授权：PR10 → PR8 → PR9 → PR11

前一检查点 5284524a7e7755c6671f804b07415dba89640cea 保留原头和 b27e164 两侧祖先；
提交门禁验证冲突改动为 5 文件、49 行，没有跳过检查。
继续仅合入新授权的固定 PR8 头 8261a07e77db36f5c8c0d5f32a2f61fb96bbe388；
其自身已包含 b27e164，完整验证由协调者继续执行，不能视为已最终放行。
两处实际冲突记录在源码外 merge-pr8-8261a07e-first.log。

App 连接入口采用 PR8 的私有 ConnectionRequest 构造边界，
保留其主机登记及六状态模式检查，不恢复旧的直接 Config 构造路径。
错误后台单元测试只在 RPC 私有模块内使用 from_config，覆盖全部七种角色。
RPC 自动合并保留 CatalogReader 读限制、来源绑定和 DraftReader 只读规则；
生产连接不暴露 connect、proxy 或裸 Config 构造入口。
先提交本轮冲突检查点，等待协调者提供 PR9 新修复固定完整头后再衔接。
在纳入该 PR9 头之前，不启动下一轮完整验证或宣称当前交付已通过。

前置库测试首轮 284/296 通过，失败原日志另存且未覆盖。
手动命令遗漏仓库规定的 macOS TMPDIR=/private/tmp，导致 /var 别名相关断言失败；
使用规范临时目录重跑全部 12 个失败项及连接、目录、草稿回归，共 36/36 通过。
该结果仅覆盖局部组合回归，不替代 PR9 衔接后的最终完整验证。
两侧测试绑定并集保留 830 项、全部 31 条需求，无删减旧测试绑定。

## 衔接固定 PR9

新授权仅合入 1810acd3a0a53cee7be3bf25af4a31ad5b1339bc；
它包含固定 PR8 和 PR10，PR8 已由协调者确认完整门禁通过（759.295 秒）。
PR9 自己的审阅与完整验证仍待协调者放行，不能由本作者代签。
四处合并冲突的首次日志为源码外 merge-pr9-1810acd3-first.log。

远程快照同时保留目录项目归属、目录只读和原生位置尚未确认时的只读约束；
补强原有回归，证明已发现但尚无 sectionVersion 的行也禁止修改和归档。
保留缓存存在性与目录新鲜度的全部断言，不删除任何旧用例。
分区逻辑目录复用 PR9 mapping::seed，它保留拥有所有权的 atoms 变量后才借用账号项；
继续支持 PR11 合并当前账号持久化 atoms 的行为，不恢复仅 owner 的旧路径。
归档 fixture 同时保留未知方法 JSON-RPC 错误回执和归档成功但回执丢失的模拟。
错误后台拒绝测试扩展覆盖 CalibrationWriter，保留 CatalogReader 与 DraftReader；
所有八种角色继续经 PR8 私有 ConnectionRequest 边界构造业务连接。
最终源码提交并写 ready.json 后，再使用固定 checker / base 对该确切头运行完整验证。

绑定并集审计发现 PR9 改名替换了旧的首次远程移动用例，故恢复原测试名及绑定。
新 fixture 先校验真实映射目录再模拟延迟位置读回，使用 PR9 校验后的 Catalog；
保留旧的状态、位置、读取三次、仅写一次及精确请求体全部断言，
同时保留 PR9 新增的未校准禁止写入和无需重读客户端状态文件两项用例。
