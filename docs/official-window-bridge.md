# 官方 Codex 窗口接入验证

2026 年 11 月 26 日，在 MacBook 上用官方签名副本验证了不修改应用包的接入方式。目标是让看板继续使用原版 Codex 的登录、远程连接和原生侧栏分区，同时避免副本安装带来的重签和系统授权。

## 路线

启动官方应用时追加 `--remote-debugging-port=0` 和 `--remote-debugging-address=127.0.0.1`，由外部看板服务连接渲染页。调试端口通过 `Runtime.addBinding` 建立一条从页面到看板服务的请求通道，再把现有的 `dist/runtime/preload.js` 注入官方窗口。

注入层只提供 `electron.ipcRenderer` 垫片，侧栏入口、看板页面、会话读取和拖拽逻辑继续复用现有运行时。看板服务负责文件持久化，并通过 `desktopWorldSource` 调用官方窗口的原生侧栏接口。

启动器 `~/Applications/Codex 看板.app` 只是执行 `official start`，再聚焦官方应用。它不复用、不覆盖官方应用包。

## 实测结果

| 项目 | 结果 | 证据 |
| --- | --- | --- |
| 端口模式生命周期 | 关闭调试连接后窗口继续运行；启动器进程退出 6 秒后窗口仍在，调试入口仍可达 | `after-client-close`、`after-launcher-exit` |
| 官方签名副本 | 深度严格验签通过，`app.asar` 与官方包摘要一致 | SHA-256 均为 `c6488597167abbbeff5a2422cdbe63f7466143b9770a224c9bc6ac2168631f13` |
| 现有登录与连接 | 读回已登录状态、9 个原生侧栏分区和 4 个远程主机 | `signedIn:true`、`sectionCount:9` |
| 原生侧栏入口 | 官方窗口出现「会话看板」，共 11 个侧栏目标、1 个注入入口 | `sidebar-mounted` |
| 看板页面 | 在原生主区域渲染 3 张真实会话卡、60 个按钮 | `page-rendered`，面板 1080×850 |
| 打开会话 | 从看板点击卡片后出现会话遮罩，关闭后遮罩消失、看板仍在 | `scrim:true` → `scrim:false` |
| 原生移卡 | 测试会话经 `codex-enhancer:board-desktop` 移入「进行中」，读回分区名称为「进行中」 | `thread/read` |
| 清理 | 测试会话已归档，活动列表只剩原有 3 条 | `thread/list archived:false` |
| 页面重载 | `Page.reload` 后看板服务重新注入，侧栏入口和运行时恢复 | `item:true`、`host:true` |
| 启动器 | 生成的启动器通过 ad-hoc 验签，重复执行能复用正在运行的官方窗口 | `codesign --verify --deep --strict` |

实验使用 `/Applications/ChatGPT.app` 的逐字节副本和独立 Electron 用户目录，未退出日用窗口。副本沿用本机 `~/.codex` 的登录和连接。

## 与管道模式的区别

上一轮原型使用 `--remote-debugging-pipe`。最后一个持有管道的进程退出时，官方窗口随之退出，需要常驻进程保管文件描述符。端口模式由 Chromium 自己持有监听端点，看板进程只作为客户端连接，断开不影响窗口生命周期。

## 尚未完成

- 拖拽手势本身没有在真实卡片上执行。合成测试会话不会进入桌面 `thread/list` 的结果，直接拖真实卡片又会改动你的会话记录，因此只验证了拖拽下游的原生移卡通道。
- 没有在日常应用包上启动，因为需要先完全退出当前 Codex。
- 官方自动更新后需要退出并重新通过启动器打开，调试参数不会由更新程序保留。
- 回环调试端口对同用户进程可见，目前只把端口文件和启动记录限制为 `0600`。
- 钥匙串、系统权限、原生 Computer Use、App Tools 和实际自动更新仍需逐项验收。
