# E3 十卡浏览器验收

owner: E3 浏览器验收作者
execution_mode: isolated_write
allowed_write_set: scripts/check-ten-card-archive.mjs、scripts/ten-card-archive-fixture.mjs、scripts/ten-card-archive-restore.mjs、tests/ui_layout.rs 和 acceptance/bindings.json 的测试入口、本卡
base_commit: d39adc926f1333c11854bcc4e50c7f2a52d78a9e
worktree_path: /home/herman/.codex/worktrees/e3-ten-card-browser/notion-program
coordinator: 五轮重构总协调

认领 `test::ui_layout::ten_card_archive_recovers_after_closing_the_submitting_page`，沿用 `browser-client.mjs` 的隔离 Chromium 与 `browser-actions.mjs` 的指针点击。HTTP 夹具在第七次归档请求保留响应，关闭标签页后保留服务端批次供新浏览器配置读回。

直接运行 `node scripts/check-ten-card-archive.mjs` 输出 JSON，其中有三个阶段的逐项详情、逐卡归档 HTTP 次数和核对请求流水。也可运行 `cargo test --locked --offline --test ui_layout ten_card_archive_recovers_after_closing_the_submitting_page -- --exact`。

产品首件 `161e983` 的上一版 `735b368` 将仍在提交的第七张卡显示为“待核对”。脚本的可选参数接受导出的页面文件，可以用同一场景复核这项差异。Mac 原生写入次数由原生验收作者提供。

重开后实际点击新输入卡的“打开会话”和“确认待归档”，后者只保存当前输入版本。核对成功时立即记录卡片的可用动作，再点击批量归档，检查新批次是否只选中仍可归档的五张卡。夹具在建批阶段截住这次操作，原十卡批次的归档 HTTP 次数可独立核算。

`161e983` 和 `b1ced225daf74b520975eb091cfa8f57595a5e8a` 均重现同一处不一致：核对后明细已将第七项计入五张已确认卡，卡片仍有可点击的“归档”。在 `bf95264` 上实际点击列头，建批请求选入六张，包含刚核实的第七项。`verified.cards` 和 `nextBatchRequest` 可对照按钮与实际名单。

`delayedRestore` 另外记录两次独立的十卡操作。两次都在第二次归档请求处关闭原标签，让新页面先收到会话列表，再扣住批次读回。分别点击单卡、批量按钮。`b1ced22` 两次都把仍待核对的原卡编入新批并再次发送归档。`bf95264` 在等待期间没有写请求，读回后实际点击分别选中一张、八张可写卡。第七项核实后的按钮问题在该产品提交上仍可复现。

产品提交 `5382001` 通过同一组断言。原批次关闭前发出七次归档 HTTP 请求，重开自动核对一次、手动核对两次后仍是七次。第五张核实已归档后，下一批实际只选中其余五张。两个慢读场景在读回前均为零次写请求，读回后单卡选中一张、批量选中八张。新增测试只绑定 `MANUAL-001`。

扩展场景覆盖核实之后新发起的快照请求。点击“刷新”后，HTTP 夹具在这次会话列表中返回 `mac` 主机仍可归档的旧修订，`other` 主机保留相同 UUID 和三个相同修订字段。`5382001` 和组合分支修复前的 `0a43708` 都把 `mac` 旧卡再次编入下一批。`d39adc9` 只选中五张，保留 `other` 卡。另一次刷新分别改变状态版本、输入版本、原生位置版本，三张卡都恢复可操作；再读旧批次后仍然可用。逐次响应、卡片动作和请求序号见 JSON 中的 `staleSnapshot` 与 `restoredRevisions`。
