# 栏目独立排序

owner: codex-column-sorting
execution_mode: serial_write
allowed_write_set: src/store.rs, src/store/sorting.rs, src/app.rs, src/mcp.rs, index.html, tests/, scripts/bind_rust_tests.py, acceptance/bindings.json, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, Docs/COLUMN-SORTING.md, context/, outputs/column-sorting-20261107/, 日常二进制与既有插件部署副本
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

后端设置及三项回归已提交 9e9e174，版本 0.1.83。
界面加入各栏独立下拉框、统一排序比较、未知时间后置与稳定身份决胜。
保留栏头控件和阅读位置，主动换序回到该列顶部。正在验证并准备部署。
界面及真实 HTTP/MCP 回归已提交 dcd2452，版本 0.1.84。
独立审查确认刷新、冲突、失败回退、拖动和窄屏行为；修复回包抢走搜索焦点。
隔离浏览器验证默认值、单栏设置、空列、筛选、搜索、刷新持久化及滚动位置。
