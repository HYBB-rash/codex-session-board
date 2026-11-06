# PWA 日常看板部署

用户授权部署 main 当前版本，供其安装为 PWA。
按仓库规则准备部署记录补丁 0.1.30，功能代码沿用 48ee1e1。
部署前备份原二进制、生产配置、SQLite 在线快照、marketplace 与旧安装缓存，
保存在 outputs/pwa-live-20261107/，目录权限 0700。

复用原配置 outputs/rust-rewrite/native-live/config.json，
端口保持 18767，数据目录保持原路径；启动器核验旧进程并确认停止。
原工作区 bin 程序原子替换后启动新服务。marketplace 与安装缓存的清单
统一为 0.1.30，保留 CODEX_SESSION_BOARD_CONFIG 注入。
通过 skills/list forceReload 与 config/mcpServer/reload 刷新客户端插件。
核对服务与安装缓存二进制 SHA256 均匹配服务 build。

实际服务保持 118 条会话，逐 ID 核对原有备注全部保留，
扫描、运行观察及状态分区同步正常。
Chrome 用户浏览器打开 http://127.0.0.1:18767/，
真实“安装应用”按钮可见，Page.getAppManifest 无错误，
Page.getInstallabilityErrors 为空，Service Worker 正常控制页面，六列显示正常。
没有代替用户安装应用，浏览器页面保留给用户自行操作。
截图：/home/herman/.codex/visualizations/2026/11/06/01a1b0ad-f2be-7113-8464-6d235f7c1e52/pwa/deployed-0.1.30.png。
此次实测只签收 PWA 部署与安装入口，完整 31 条验收仍需其他环境证据。
独立查询 MCP 服务器确认加载 0.1.30 且提供看板工具；生产配置逐字节不变，
116 条人工状态保留，实际 PWA 资产与 main 源文件一致并带 no-store。
完整验收报告 acceptance.json 的 254 项自动检查通过，退出 2，
release_ready=false。提交前执行统一检查与测试钩子，提交后执行普通检查。
