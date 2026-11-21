# 批量归档页面

维护者修改逐项状态时从 `assets/board-archive-batches.ts` 的 `status` 开始。服务端批次提供已确认、待核对、新输入、拒绝及尚未提交。页面只为本次发送区分正在提交和排队。`assets/board-archive-results.tsx` 渲染摘要、明细、重读和核对动作。原先手工拼装节点、保存渲染签名、重绑按钮和恢复明细的分支已删除。

### 实际读回

| 十卡浏览器夹具 | 结果 |
|---|---|
| 第七项请求尚未收到确认时关页 | 已发出 7 个归档 HTTP 请求 |
| 新浏览器重开并读取持久批次 | 4 项已归档，1 项待核对，3 项尚未提交，1 项有新输入，1 项被明确拒绝 |
| 两次手动核对 | 首次仍待确认，第二次读回后共有 5 项已归档，累计仍为 7 个归档 HTTP 请求 |
| 首读比会话列表返回得晚 | 单卡和批量按钮都不能发起写入。首读失败可重新读取，未提交卡随后恢复可操作 |

独立审阅复现了两个重复提交窗口。重开时，页面现在等批次首读结束才允许提交。收到核对结果时，页面按完整会话 ID 暂停受影响卡片的归档，并让旧的在途快照失效。若下一次读回失败，其他主机的同 UUID 卡仍可操作。读回成功后，页面直接采用根快照。

旧批次退出最近二十批后，根库仍能识别已确认的原修订；这项判断由 `src/store/archive_batches.rs` 的 `archive_receipts` 供快照和写入共用。`test::ui_layout::ten_card_archive_recovers_after_closing_the_submitting_page` 保护重开页面的人。第七次归档发出后关闭页面，核对期间旧 GET 晚到、新 GET 返回 503，再点一键归档只能选到五张。临时移除请求序号保护时选到六张。目标库存储写入失败后的旧修订、其他主机、历史记录和原生恢复见 [根库读回回归](archive-confirmed-window-20261121.md)。

`test::ui_layout::long_content_and_project_filters_preserve_browser_column_geometry` 现在点击真实的筛选后一键归档按钮，检查批次选中的卡和每条请求的三个版本字段。等价地改写按钮函数后通过。故意累积旧监听会发送上一个主机的卡，放宽批量资格会夹入同一项目的只读卡，两种故障都触发请求名单断言。`test::archive_flow::archive_mcp_resource_exposes_only_an_explicit_app_action` 保留工具的可见性、写入属性和必要参数，逐字匹配按钮 JavaScript 的断言由上述真实操作承接。

原生十卡夹具逐卡记录 `thread/archive` 调用。两次重开、核对及重放后，十项累计写入次数依次为 `1、1、1、1、0、0、1、0、0、0`。后端用例为 `ten_card_batch_reopens_from_storage_and_checks_without_repeating_native_writes`。
