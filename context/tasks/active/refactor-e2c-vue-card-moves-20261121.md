# E2c 拖动卡片与跨列保存

- owner: E2c Fedora 作者
- execution_mode: isolated_write
- coordinator: Herman / 01a1fc2d-af6f-7d50-9759-33835b51eaeb
- base_commit: e63375e26f9e8fcebde41514af6181afddd5f4bb
- worktree_path: /home/herman/.codex/worktrees/e2c-vue-card-moves/notion-program
- branch: codex/e2c-vue-card-moves
- allowed_write_set: Vue 卡片与列的拖动事件与视觉反馈、旧拖动监听清理、浏览器拖动场景和本项记录。

E2b 已提供可交互编辑器和草稿恢复。E2c 在松开鼠标前保存拖动起始时的会话身份和版本。拖入阻塞列打开编辑器，其余跨列动作提交到 A 的保存入口。新输入、拒绝、计数、筛选、焦点与滚动从浏览器和真实的隔离后端核实。最终提交待 E2b 进入主线后重基。
