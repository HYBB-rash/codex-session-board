# E3 十卡回放：旧读取晚到与根快照恢复

owner: E3 十卡夹具作者
execution_mode: isolated_write
allowed_write_set: scripts/check-ten-card-archive.mjs、scripts/ten-card-archive-fixture.mjs、本记录
base_commit: a829e5582fd82e585d2dc52596777064983db05f
worktree_path: /home/herman/.codex/worktrees/e3-ten-card-browser/notion-program
coordinator: E3 组合作者

`test::ui_layout::ten_card_archive_recovers_after_closing_the_submitting_page` 通过真实点击安排两次会话读取。第一次在核实前发出，夹具保留当时可写的旧行和响应；收到核实回执后才允许它送达。核实触发的新读取返回 HTTP 503。健康提示显示读取失败时，原卡仍可见且没有归档按钮，另一主机的同 UUID 卡仍可用。点击列头只选中五张。

手动重试成功时，夹具按根快照契约省略已确认的活动修订。后续快照分别恢复状态版本、输入版本或位置版本有变化的卡。原批次始终只有七次归档 HTTP 请求，一次自动、两次手动核对。单卡和批量慢读期间均无写请求，读回后分别选中一张、八张。

负对照使用同一份 HTML，仅在临时副本中让 `load()` 采纳旧序号的成功响应。相同 Chromium 回放捕获到原卡的归档按钮重新可用，列头建批请求选入六张。完整请求和页面动作分别记录在 `lateSnapshot`、`nextBatchRequest`、`rootSnapshot`、`restoredRevisions` 中。原生归档写入由后端作者取证。
