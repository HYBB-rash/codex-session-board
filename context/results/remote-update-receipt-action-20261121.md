# 远端保存回执的操作分派

`App::execute` 将已解析的 `Operation` 传给子实例。远端保存按 `Action::Update` 生成根实例的整卡回执。远端写入资格、目标裸线程 ID 及请求正文继续经过原有检查。

## 路径改名对照

在 `b615d4d` 的独立源码副本中，把公共 Update 登记改为 `POST /api/revised-update`。隔离 HTTP 服务连接两台含相同 UUID 的原生接口 fixture，提交至 mac，随后读回根实例的两张卡。旧路径请求失败。

| 同一登记与 HTTP 请求 | mac 保存结果 | HTTP 回执 | 另一主机 |
| --- | --- | --- | --- |
| 修复前 | 状态、备注已写入 | 裸线程 UUID，与根实例整卡断言不符 | 原卡未变 |
| 修复后 | 状态、备注已写入 | 与根实例读回整卡完全相等，含 mac 的复合 ID、`hostId`、`threadId` 及版本 | 原卡未变 |

对照日志、两次实际响应和快照在工作树旁 `evidence/route-change/`，分别为 `http-before.log`、`http-after.log` 和 `remote-receipts.json`。正式登记表仍使用原路径。

## 定向验证

- `managed_write_gate_requires_initialization_and_preserves_notes_after_home_change` 增加已解析 Update 改名后整卡回执断言。旧实现返回“接口不存在”，修复后通过。其余连接守卫改由公共请求入口进行验证。
- `remote_online_disconnected_and_scanning_do_not_delay_local_http_mutations` 在正式路径上核对在线远端保存、整卡读回以及同 UUID 的另一主机。
- 生产改动位于 `src/app.rs` 和 `src/app/remote.rs`，无需改动 HTTP、MCP 登记、私有校准端点或归档格式。此包与 Vue 筛选分别提交。
