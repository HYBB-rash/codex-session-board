# PR 6 集成兼容与侧栏归属修复

- owner: codex-original-pr6
- execution_mode: isolated_write
- allowed_write_set: 本任务记录；fixture 托管、实际包启动验证、侧栏归属及相关回归
- base_commit: 865fb97
- worktree_path: /Users/hongyong/.codex/worktrees/package-authority-fix/codex-session-board
- coordinator: 01a1d38b-1ef8-7392-ba78-caa7935c5ab4
- status: ready_for_coordinator

固定主线 04f7d58 与原 PR 头 9667957 无冲突组合为签名复现基线 865fb97。
先让包内 serve 与 MCP 均由监听端点的同一 fixture 托管，保留原有身份、就绪、协议与清理断言。
随后封闭实时后台与侧栏来自不同客户端的写入边界；不能证明归属时，在任何写入前拒绝。
不修改协调者工作区、共享 STATE、版本或日用客户端。仅执行针对性验证；完整双平台候选由协调者统一执行。

包托管实现检查点 9ac11e8；侧栏归属与共享客户端证明检查点 af7082a。针对性第二轮
81 项 RPC、37 项原生、3 项 fixture、29 项 Python 及相关 Clippy 通过；另 49 项远程回归通过。
侧栏首轮新增回归发现上层把来源拒绝包装为移动未确认，已将来源校验提前到连接阶段并复验，
保持原生成功路径与错误来源零字节断言。共享兄弟进程必须有后台、GUI、启动器三方身份及
实际父子关系；普通 stdio 客户端按后台与 GUI 父子关系核对。旧 epoch 和重启拒绝已有回归。

新增凭证入口的真实 Mac 私有验证通过，后台 98759、GUI 98770、启动器 98731；清理确认。
首个实际包 af7082a 已构建、归档并解包；HTTP 和 MCP 均成功，但比较身份时误把 HTTP 专用
requestToken 当作共享身份，导致验证拒绝。失败及 cleanup_confirmed=true 保存在
/private/tmp/csb-pr6-package-proof-20261114-r1/success.json。改为严格核对七项实际身份字段，
包括后台 PID/epoch、服务 PID、构建、配置及数据目录；不把传输令牌保存为后台身份。
实际包完整正例及故障注入仍待下一轮完成。协调者指出的 HTTPConnection 上下文协议问题
已按既有 closing 模式修正，异常原始 error 与清理字段继续保留。

## 交付给协调者

前三个代码修复提交依次为 9ac11e8、af7082a、53851fb，均已用本机 Agent 密钥签名并验证。
当前分支 codex/package-startup-authority-20261114；协调者可只带入上述三条及本任务收尾记录，
不必重放固定主线与原 PR 的复现合并基线 865fb97。

前一轮实际包绑定代码提交 53851fb8ce58ece8be108bdf006cd9c3d36d8eeb，
源码指纹 2b463d4540384589be7c5336a4536a08b260361e1a2dd4289d3c8b8df6d3506f，
二进制摘要 af0d47ef75f9a037463ec3907e7d3a9ad13aea05162b5bd392fd26012ff57c02。
私有归档经过实际解包及完整 verify()，五个场景按预期通过：

- 正例：同一 fixture 26436 启动包内 serve 26438 与实际 MCP 26529；后台启动身份一致，
  HTTP 与 MCP 的服务 PID、构建、版本、配置、数据目录和后台身份全部一致，MCP 退出 0。
- 配置在就绪后变化：真实 MCP 返回工具错误，验证拒绝；不把协议错误当作成功。
- MCP 使用无效配置：真实包内 MCP 退出 1，验证拒绝。
- 错误实时后台：60 次实际连接均收到零字节，在握手前拒绝，完整就绪断言超时失败。
- 实际 serve 使用无效配置：子进程提前退出，验证拒绝。

五个场景的 cleanup_confirmed 均为 true，保留服务、MCP、fixture 的 PID 与父进程记录，
后台启动身份、原始错误和日志。第二轮故障注入原先把配置变化的协议错误误期待为 CLI 非零退出，
该注入器失败保留；第三轮分别验证协议错误和真实非零退出，生产断言没有放宽。
最终源码的 13 项启动/发布 Python 回归再次通过。其余 Rust 与共享入口相关验证记录绑定
前一代码指纹 e42f2bf；此后只修改包验证器的身份比较、其 Python 回归与本任务记录，
Rust 及共享启动实现没有变化；最终实际包五场景绑定上述 53851fb 指纹。

持久证据：
/Users/hongyong/.codex/visualizations/2026/11/14/01a1d89d-57e0-71a2-9058-0775f92bc693/pr6-integration-fix。
SUMMARY.json 为入口，actual-package 包含原始不可变归档、逐场景报告、进程记录及注入器，
targeted 包含相关测试与最终 Python 日志，prior-r1/r2 保留原失败，mac-sharing.json 及
Mac 日志记录真实私有客户端验证。完整双平台候选门禁仍由协调者统一执行；
未推送修复分支、更新原 PR 固定头、合并主线、升版、部署或操作日用客户端。

最终复核补回原 MCP 子进程验证的 30 秒总时限：改用单一截止时间读取 Unix 通道，
避免零碎输出不断重置 socket 超时；新增慢流截止回归。此变化后将再次固定源码、
构建并执行同一组实际包场景，不能沿用上面 53851fb 的包给新验证器签收。

最终代码固定为 a0d80015f3dd6608cf9762d2c892ed29146428d4，第四条修复提交为 a0d8001。
其源码指纹 187df7c768705b8837eb4d55930eb39f979355b0dfffde7cd2ab2739387a8342；
实际私有包重新构建、归档与解包后，五个场景全部按预期通过，逐场景清理均确认。
最终归档摘要 a918dbf8fdecd6fc48625422121b199241869ce594050cdb0acc212fd25335e2。
持久证据目录的 SUMMARY.json 已指向 final-package/summary.json；
该目录包含最终原始归档、原始报告、进程身份、故障注入器及日志。
原始执行目录 /private/tmp/csb-pr6-package-proof-20261114-r4，注入器正常退出 0。
包启动和发布 14 项回归通过；此前 Rust、fixture、
转发和远程验证仍对应未变化的 Rust 实现。仅保留必要边界修复，没有运行全量验收。
协调者可依次带入 9ac11e8、af7082a、53851fb、a0d8001；之后的本任务收尾提交只改 context。
工作树按协调者要求保留，待其主线确认后通知清理；不自行推动合并或部署。

最终 30 项组合 Python 复验发现共享启动测试的模拟后台在 accept 后立即关闭连接，
Mac 的 LOCAL_PEERPID 存在偶发 ENOTCONN。旧失败日志保留为 targeted/final-python.log。
修正此模拟后台等待客户端关闭后再关闭，保持真实连接生命周期及全部原断言；
业务源码仍固定 a0d8001。只补这一处测试 fixture 修正及记录后重新固定最终交付。

最终可带入提交 be902aab76c0d57a10ac8e25648a51679d8cf589 已验证 Agent 签名，
其中 a0d8001 为最后一条业务/验证器实现变化，be902aa 只修正测试连接生命周期及记录。
最终源码指纹 022734d1938437c0d809f4b5755c76375794982a855329f9257c0a77488f76fe。
第五轮重新构建并解包该提交的实际包，五场景全部按预期通过，全部清理确认，执行退出 0；
最终归档摘要 8d601ef8d2aacb06be6c125bcd361c4d1e5e56152cd7f50c451ec522e3fca42e。
30 项相关 Python 组合复验通过，日志 targeted/final-python-r2.log；原失败日志保留。
持久 SUMMARY.json 与 final-package/summary.json 已更新为上述最终 SHA，
前一轮时限实现的实际包保存在 deadline-package；原始最终执行目录
/private/tmp/csb-pr6-package-proof-20261114-r5。业务二进制摘要与前两轮一致。
若集成分支已带入至 53851fb，只需追加 a0d8001 与 be902aa，再由协调者运行最终双平台 CI。
本任务随后仅提交此记录；共享 STATE、版本与主线仍未改动，工作树继续保留。
