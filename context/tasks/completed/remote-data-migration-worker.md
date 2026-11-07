# 远程子库迁移修复

owner: Lorentz
execution_mode: isolated_worker
allowed_write_set: src/service.rs, src/service/migration*, tests/native_authority/remote_isolation.rs, tests/native_authority/remote_fixture*, context/handoffs/remote-data-migration-worker.md, own versions and acceptance/bindings.json after coordinator base confirmation
base_commit: 918ee9e4b3d3e3c1b3ab00dcb0f9425f2ce950dd
worktree_path: /home/herman/.codex/worktrees/remote-data-migration/notion-program
coordinator: codex

创建前协调工作树干净。工作者只在独立工作树修改迁移实现及定向测试，
不修改共享登记、状态或日志，不操作真实数据。迁移补丁已完成审阅；
收到协调者最终基线后，允许在独立树准备版本、绑定测试并分步提交。
此前先升级旧远程隔离测试夹具，保留全部断言、超时及真实前台路径。
夹具仅模拟测试 SSH 和 daemon，禁止实际连接用户主机。

目标为迁移根库及全部持久主机子库，保留侧车身份与分页进度，
拒绝覆盖和符号链接，SQLite 使用在线备份，失败仅清理本次新建内容。
测试覆盖最新备注、历史、标题队列、输入确认、归档墓碑、目标冲突、
部分失败及重试。跨库快照的一致性边界须明确记录。

协调者已将 379d34a 至 b99199b 的六提交 fast-forward 接入主协调树，
版本为 0.2.26 至 0.2.31。新增 23 个迁移测试，原测试标识保留。
主库及所有持久主机子库使用 SQLite Backup，额外只复制身份和扫描进度。
配置最后发布；失败回滚只删除本事务拥有的文件，保留并发新增内容。
实现与隔离测试交付完成；真实用户数据迁移和产品发布不由本记录签收。
