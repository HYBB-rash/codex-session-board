# 2026-11-06 待归档正序

待归档列在渲染时按 updatedAt 升序排列，与卡片显示的会话更新时间一致。其他列保留原顺序。
实际 render 回归检查通过，包含搜索、刷新、同时间顺序及空列。
完整验收执行 228 个自动测试均通过；真实环境证据缺失，退出 2，release_ready=false。
报告保存在 /tmp/session-board-archive-oldest-first-acceptance.json。
普通项目检查仅因本次未提交改动退出 1，未提交、未升版、未部署。上下文检查通过。
