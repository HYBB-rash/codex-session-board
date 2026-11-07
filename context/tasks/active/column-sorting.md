# 栏目独立排序

owner: codex-column-sorting
execution_mode: serial_write
allowed_write_set: src/store.rs, src/store/sorting.rs, src/app.rs, src/mcp.rs, index.html, tests/, acceptance/bindings.json, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, Docs/COLUMN-SORTING.md, context/, outputs/column-sorting-20261107/, 日常二进制与既有插件部署副本
base_commit: abfa090
worktree_path: /home/herman/文档/notion-program
coordinator: codex-column-sorting

用户授权按调研方案实现并直接部署，随后自行验收。
各状态栏独立按会话更新时间正倒序，保持待归档旧的在前、其他新的在前。
偏好由后端按栏目保存，独立版本保护；不写入会话状态、备注或复核信息。
排序字段与方向分开，统一比较入口，为未来优先级留扩展位置。
先提交后端接口及回归，再提交界面与交互验证，最后部署并核对配置和数据。
原 31 条冻结契约保持不变，缺少的真实产品验收继续待验证。
只读子代理检查部署复用与测试风险，主代理为唯一写入者。
