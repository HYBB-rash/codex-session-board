# Codex Enhancer 技术验证报告

## 结论

**外部 Runtime 可以在 Fedora 的 Codex Desktop 测试副本里稳定加载，并在官方窗口上增加按钮和页面。**  
卸载能把 `app.asar` 恢复成与 `/opt/codex-desktop` 逐字节相同的文件。  
日常安装没有被修改，系统归档 inode 仍是 `18064179`。

测试对象是本机 RPM `codex-desktop-2026.11.19.020106-1.fc44.x86_64`。  
Codex 版本是 `26.1111.71704`，Owl 记录的运行时名称是 `owl`，`version` 文件里的 Chromium 版本是 `157.0.8103.78`。

## 验证结果

| 验证项 | 结果 |
|---|---|
| 是否识别当前 Codex 版本 | 已确认。`detect` 读到 `26.1111.71704`。 |
| 是否识别 Owl / Electron | 已确认。`owl-electron-app.json` 的 `runtimeName` 是 `owl`。主进程里 `process.versions.electron` 为 `157.0.8103.78`，与 Chromium 版本文件相同。 |
| Loader 是否成功执行 | 已确认。日志事件 `loader-entered` 和 `main-loaded` 来自测试副本的 `ChatGPT` 进程。 |
| 外部 Runtime 是否成功加载 | 已确认。`runtime/main.js` 在仓库目录，不在应用包内。`hello-world` 扩展被加载。 |
| Preload 是否成功注入 | 已确认。`session.registerPreloadScript` 注册了额外 preload，按钮出现在 `app://-/index.html`。官方 `webPreferences.preload` 没有被改写。 |
| 自定义按钮是否出现 | 已确认。登录页右上角有 Enhancer 按钮，见 [官方界面](screenshots/official-ui.png)。 |
| 自定义页面是否正常显示 | 已确认。按钮点击后打开页面，内容为 Codex Enhancer、Runtime loaded successfully、Codex Version: 26.1111.71704、Runtime Type: owl，见 [自定义页面](screenshots/custom-page.png)。 |
| 原有 Codex 功能是否正常 | 已确认窗口能显示官方登录页，主框架 `ready-to-show`，路由挂载，app-server 应答了 `config/read` 和 `account/read`。未用日常账号打开已登录会话。 |
| 卸载是否完整恢复 | 已确认。两次卸载后的 `app.asar` SHA-256 都是 `20c53634196da5d0a84b7cc5c8113a7370aa4ec0b78fffeb6cda3c03f138301e`，与系统文件 `cmp` 一致。恢复后的副本能启动，且不再写 Runtime 日志。 |
| Codex 更新后是否能够检测到 Patch 失效 | 已确认检测逻辑。改副本版本号会得到 `copy-version-changed`。用原始归档替换已打补丁的归档会得到 `archive-hash-changed` 和 `loader-missing`。没有实际执行一次 dnf 升级。 |

补丁后的归档 SHA-256 是 `4d3d605edce35c34d2b8104b775d3e38742f680a36760fd08ad0aabf8de36ddc`。  
同一份原始归档重复安装，补丁哈希不变。

## 已确认

**启动入口是从归档里读出来的，不是写死的文件名。**  
`resources` 下唯一带 `package.json` `main` 的归档是 `resources/app.asar`，入口是 `.vite/build/early-bootstrap.js`。  
补丁只在这个入口前面加上 Loader，`package.json` 的字节保持不变，入口其余字节与备份一致。

**Loader 通过测试副本里的链接文件加载仓库外的 Runtime。**  
链接要求 Runtime 路径位于 Enhancer 目录之内。  
主进程记录了版本 `26.1111.71704` 和运行时 `owl`，并通过 `ipcMain.handle("codex-enhancer:info")` 把这两项交给页面。

**额外 preload 加在会话上，官方 preload 路径仍由应用自己传入。**  
按钮只挂到 `app:` 页面。  
内嵌的 `codex-sandbox:` 页面不再挂按钮。  
验证运行里，按钮的 `click` 事件打开了自定义 `BrowserWindow`，页面 preload 填入版本和运行时类型。

**完整性 fuse 保持原状。**  
二进制 fuse 为 `RunAsNode=off`，`EnableCookieEncryption=on`，`EnableNodeOptionsEnvironmentVariable=off`，`EnableNodeCliInspectArguments=off`，`EnableEmbeddedAsarIntegrityValidation=on`，`OnlyLoadAppFromAsar=on`，`LoadBrowserProcessSpecificV8Snapshot=off`，`GrantFileProtocolExtraPrivileges=off`，`WasmTrapHandlers=on`。  
补丁重算了被修改文件的 SHA-256 和 4 MiB 分块哈希。  
在 fuse 不改的情况下，测试副本完成了启动、登录页渲染和页面打开。

**隔离生效。**  
启动日志写明用户数据目录选中了 `state/user-data`，并忽略了之后改到 `/home/herman/.config/Codex` 的请求。  
`CODEX_HOME` 指向 `state/codex-home`，应用 ID 是 `codex-enh-poc`。

## 推测

**这个 Linux 构建没有执行 macOS 和 Windows 那种嵌入式 ASAR 头哈希校验。**  
依据是 fuse 虽然打开，二进制里找不到 `ElectronAsarIntegrity` 字符串，归档头被重写后进程没有因完整性失败退出。  
这与上游 Electron 文档一致：嵌入式 ASAR 完整性目前实现在 macOS 和 Windows。  
Owl 是否在别的版本把同一检查编进 Linux，这次不能外推。

**官方 preload 仍在运行。**  
依据是注入使用的是追加 API，官方页面仍能完成自己的启动和登录界面渲染。  
这次没有单独打印官方 preload 的文件路径。

## 未验证

**没有用日常登录态打开真实会话。**  
测试副本使用空的用户目录，避免写入 `~/.config/Codex` 和 `~/.codex`。  
因此不能把“登录页和 app-server 握手成功”说成“已登录后的对话功能已验证”。

**没有安装一个更新的 Codex RPM。**  
`status` 能发现归档被替换、Loader 消失和版本号变化。  
真正的 dnf 升级是否换成新的入口文件名，要等下一次系统更新再跑 `status`。

**没有故意写入错误的分块哈希来观察 Owl 会不会拒绝读取。**  
当前补丁写入的是正确哈希，文件被成功执行。

## 不支持

**不能靠 `NODE_OPTIONS` 或 `--inspect` 注入。**  
对应 fuse 是关闭的，这次也没有尝试绕过。

**不能把 `app.asar` 换成同名目录。**  
`OnlyLoadAppFromAsar` 是打开的，所以补丁保持归档形态。

**这次没有证明 Windows 或 macOS 可以沿用同一补丁。**  
公开的 Codex++ 资料说明 macOS 需要重写 `Info.plist` 里的 `ElectronAsarIntegrity` 并重新签名。  
本机没有做这件事，也没有关闭 fuse。

## 实现要点

系统安装由 `/usr/bin/codex-desktop` 解析到 `/opt/codex-desktop/start.sh`，再定位 `ChatGPT`。  
测试副本的启动入口被加上一段同步 Loader。  
Loader 失败时只打印错误，后面的官方程序仍会执行。  
Runtime 在主进程注册额外 preload，并用固定频道和页面通信。  
扩展目录目前只有 `hello-world`，它提供按钮文字。  
以后可以继续往 `src/extensions/` 增加本地扩展，而不把扩展代码打进 `app.asar`。

参考实现的公开设计来自 Codex++。  
legacy 1.x 把外部 Runtime 和 `session.registerPreloadScript` 作为追加 preload 的做法，和这次验证成功的路径一致。  
本次代码是按这个 Fedora 包重新写的，没有复制该仓库源码。该仓库许可证是 MIT。
