# 卡片显示所属项目

owner: codex-project-card-label
execution_mode: serial_write
allowed_write_set: index.html, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, project-rules.json, context/, outputs/project-card-label-20261107/, 日常二进制与既有插件部署副本
base_commit: 7bec275ae2664ce6da12c9edc0269ed2c7117b54
worktree_path: /home/herman/文档/notion-program
coordinator: codex-project-card-label

用户确认沿用项目筛选器的归属，为卡片增加项目标签，补齐项目名搜索并直接部署，
随后由用户验收。复用项目 ID、项目目录和既有工作树归属，不新增持久化字段。
处理默认工作区、同名、远程主机、项目资料过期与归属异常。
标签支持展开完整名称，保留原路径和点击、拖拽行为。配置和数据原样保留。
完整产品验收缺失的外部证据保持待验证，不以本次部署代替完整验收。

已部署 0.1.82，等待用户验收。标题下显示项目标签，与顶部筛选器复用项目名称
及同名消歧；默认工作区和远程主机分别标识，旧资料与归属异常显示待确认。
原生 details 支持鼠标、键盘及窄屏展开完整名称和目录，刷新保留展开及焦点。
展开标签不会打开会话，路径、卡片跳转、拖拽及归档接口保持既有行为。
搜索新增项目名匹配。

23 项隔离边界检查通过。完整冻结门禁执行 322 项自动检查全部通过，
17 条需求通过、14 条待外部证据，退出 2，release_ready=false。
浏览器验证同名、默认、异常、远程标签、仅项目名命中、390 像素窄屏展开
以及周期刷新保留详情。真实页面 51 张卡片都有标签，筛选和搜索均返回
Notion任务看板管理的 5 张卡片，页面没有控制台错误。

程序、marketplace、插件缓存、实际运行程序的 SHA256 一致。
HTTP、插件安装和客户端 MCP 均为 0.1.82；配置与注入 MCP 环境逐字节保留。
部署前一致性备份后，核对 132 本地、42 远程人工字段和 1 条设置保留，
输入版本没有倒退。沿用每 30 分钟定时归档配置，保留既有单条位置缺项提示。
证据、截图及回滚脚本位于 outputs/project-card-label-20261107/。
