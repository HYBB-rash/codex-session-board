# Codex 原生 UI 集成调查

证据按这五档写。  
**已运行**指测试副本里发生过。  
**源码确认**指从当前 `app.asar` 或 [Codex++](https://github.com/b-nnett/codex-plusplus) 仓库读到。  
**运行时推测**指日志或画面只给出方向。  
**尚未验证**指代码在，这次没有走到。  
**当前版本不支持**指这份安装或那个仓库没有这条能力。

## 结论

**Session Board 没有进入官方侧边栏。**  
测试副本停在登录页，文档里没有导航轨，宿主也就没有按钮可挂。  
同一轮里，宿主已经在官方主窗口的 `app://-/index.html` 中运行，并在轨出现前保持等待。  
日常安装 `/opt/codex-desktop` 和日常配置没有改。  
启动注入仍是 0.1 的外部 Loader，没有关 Fuse，也没有改官方业务包。

[登录页截图](screenshots/login-shell-absent.png) 是 2026-11-22 16:23（UTC+8）从测试副本主窗口抓的。  
画面是「登录 ChatGPT」，带「继续登录」「使用其他方式登录」和「注册」。  
没有侧边栏，也没有 0.1 那个悬浮按钮。

## 这次跑过的测试副本

**已运行：新进程加载了外部 Runtime，并在官方页面里执行了 Preload。**  
16:23 的日志依次是 `loader-entered`、`session-board-loaded`、`main-loaded`、`preload-registered`、`preload-entered`。  
`main-loaded` 记下的版本是 Codex `26.1111.71704`，运行时 `owl`，Chromium `157.0.8103.78`。  
扩展列表是 `hello-world` 和 `session-board`，已登记的侧边栏 id 只有 `session-board`。  
`preload-entered` 的地址是 `app://-/index.html`，`window.top === window`。

**已运行：登录页稳定后仍然没有壳层标记。**  
`shell-observed` 在进入页面时和 4 秒后各记一次。  
两次都是 `rail: 0`、`destinations: 0`、`workspace: 0`、`pageSidebar: 0`。  
没有 `sidebar-mounted`，也没有 `page-rendered`。  
所以这次不能把「入口已在侧边栏里」当成已验证结果。

**已运行：沙箱页面的 IPC 被拒绝，主页面的证明消息被接受。**  
可视化页的地址协议是 `codex-sandbox:`，它自己的 `parent` 是 `null`。  
该帧发出的 `codex-enhancer:proof` 记成 `ipc-rejected`，没有写入渲染证明。  
主页面的证明被写入。  
校验只认两件事：帧对象带有 `parent` 且值为 `null`，地址协议是 `app:`。  
单测覆盖了子帧和缺 `parent` 的拒绝。  
路径还没有收成 `/index.html`。顶层 `app:` 页面目前都能要到侧边栏清单。

**已运行：沙箱 Preload 不能 `require` 旁边的文件。**  
拆开的 `preload.js` 在运行时报 `module not found: ./host.js`。  
构建现在把注册表和宿主拼进同一个 `runtime/preload.js`。  
拼完的文件没有 `./host.js` 或 `./registry.js`，随后 `preload-entered` 出现。

**已运行：观察者必须等文档根节点出现。**  
Preload 一开始就调用 `MutationObserver.observe(document.documentElement)` 时，根节点还不存在。  
错误是 `parameter 1 is not of type 'Node'`。  
现在等 `DOMContentLoaded` 再观察。  
隔离世界里 DOM 的 `instanceof` 不可靠，节点判断改成看 `getAttribute`、`style` 和 `tagName`。

测试命令 `npm test` 在这次构建上通过，7 项，含归档、重复 id、非法 id、路由和 IPC 帧。

## 官方界面怎么组成

**源码确认：功能导航是壳层包里的一条轨，和会话列表不是同一块。**  
壳层包是 `webview/assets/async/14459.bfda23f585.js`。  
轨的标记是 `nav[data-app-navigation-rail]`。  
无障碍名来自消息 `sidebarElectron.appNavigation`，默认英文是 `App navigation`，当前中文包是「应用导航」。  
选择器不能用这段英文。  
目的地按钮带 `data-sidebar-destination`，值是目的地对象的 `id`。  
键盘移动使用轨内的 `button:not(:disabled)`。  
同一包里的工作区标记是 `data-app-shell-workspace`，页面侧栏是 `data-app-shell-page-sidebar`，外框是 `data-app-shell-frame`。  
会话行 `data-app-action-sidebar-thread-row` 在 `907538.b6edd11ef3.js`，不在这道功能轨上。

**源码确认：目的地是编译进包里的目录，没有第三方注册函数。**  
属性值来自 `b.id` 这类变量，不是写死的一个字符串。  
同一包里能读到的 id 包括 `builtin:home`、`builtin:codex`、`builtin:library`、`builtin:sites`、`builtin:orbit`。  
哪些 id 会画出来，还要经过同一包里的产品模式条件。  
这次登录页没有轨，所以当前账号会显示哪几项尚未验证。  
`onSelect` 走应用内部导航。包里没有给外部代码追加目的地的导出。

**源码确认：设置页有自己的路由和内部目录函数。**  
模块 `OSCq` 的路径是 `/settings`。  
匹配函数接受 `/settings` 和以 `/settings/` 开头的路径，并带 `settingsLayout: true`。  
`settingsNavigationSections` 由设置包里的内部函数返回。  
读到的一处侧栏项把 `data-settings-panel-slug` 写成字面量 `teams`。  
中文消息里有 `settings.nav.general-settings`、`account`、`appearance`、`keyboard-shortcuts`、`personalization` 等固定 id。  
没有读到扩展可以调用的设置注册导出。

**源码确认：主页面不能把外部文件嵌进 iframe。**  
`webview/index.html` 的 `default-src` 是 `'none'`。  
`script-src` 只有 `'self'`、一段固定哈希和列出的外部脚本。  
`frame-src` 没有 `file:`。  
`style-src` 允许 `'unsafe-inline'`。  
因此页面只能由 Preload 在当前文档里建节点。另开 `file:` 文档或再插一个脚本标签，会被这份策略挡住。

**源码确认：Owl 能创建主渲染 `WebContentsView`，这次没有把它当成扩展接口。**  
`.vite/build/early-bootstrap-100-449e39ca9c7eadbd.js` 的 `createAppView` 用 `getRendererPreferences("primary", path)` 构造 `WebContentsView`。  
同一文件有一次 `contentView.children` 查找，条件是子节点属于 `WebContentsView`。  
该文件没有 `addChildView`、`setContentView`、`BaseWindow`。  
`createAppView` 这个名字在文件里只出现在定义处，挂到窗口上的那一行这次没有定位到。  
已运行的窗口事实只有两条：主文档是 `app://-/index.html`，可视化页是另一个顶层 `codex-sandbox:` 页面。

**已运行：登录页替换的是整页，不是把轨藏起来。**  
截图和 4 秒后的计数都是 0。  
「继续登录」对应消息 `electron.onboarding.login.chatgpt.continueToSignIn`。  
这是引导登录，会离开当前页去完成账号登录。  
测试副本没有完成这一步。日常会话也没有复制过来。

## 三种扩展方式

| 方案 | 判断 | 依据 | 维护成本 |
|---|---|---|---|
| DOM 注入 | 这次采用。登录后轨出现时可以挂按钮，并在当前窗口画页面 | 已运行：宿主在主文档等待。源码确认：轨和工作区标记都在壳层包 | 标记或类名一变就要改宿主。观察整棵文档 |
| React 组件复用 | 当前包没有可调用的组件或路由注册口 | 源码确认：组件在哈希包里，类名也是哈希。设置目录是内部函数 | 要改官方包才能把节点插进 React 树。版本一变补丁就失效 |
| Owl View | 能新建一块原生视图，不能靠它登记侧边栏项 | 源码确认：有 `WebContentsView` 构造。没有读到侧边栏注册。挂载行未定位 | 私有接口。布局仍由 React 决定，多一块视图对不齐轨 |

**已决定：最小实验走 DOM 注入，不改官方业务包。**  
按钮优先挂到轨外、带可见文字的 `data-sidebar-destination` 旁边。  
没有这种行时挂到 `nav[data-app-navigation-rail]`。  
页面是 `document.body` 上的 `section`，按工作区的位置和计算样式固定盖住内容区。  
Back 移除这一层。点击轨上其他目的地也会移除。  
`route` 只保存在注册表里，没有写进官方历史。  
`icon` 也只保存。宿主画的是固定四格图标，没有接到官方图标库。  
React 重新渲染时，观察者按当前清单把按钮补回去。  
这些挂载、翻页和补回步骤都尚未在有轨的实机页面上点过。

## Codex++ 能复用什么

材料来自 [Codex++ 2.1.0](https://github.com/b-nnett/codex-plusplus/blob/main/VERSION) 的仓库源码，发布记录是 2026-10-15。  
`Package.swift` 只声明 macOS 14。  
钉死的适配器是 ChatGPT `26.1007.21159`（build 20052，框架 `155.0.8059.40`）和 `26.1002.52244`（build 13536）。  
README 另写审核过的包是 ChatGPT `26.1007.71326`，build 20398，arm64。  
这些都不是本机的 Codex Desktop `26.1111.71704`。  
本机没有运行它的安装器，也没有把它的补丁打进 Fedora 上的测试副本。  
下面凡是「仓库源码」的句子，都还没有在这份 Owl 构建上复现。

| 题目 | 仓库里的状态 | 这份 Fedora 构建 |
|---|---|---|
| 设置导航 | 已实现。用 Acorn 改官方设置模块，插入固定 slug `codexpp-extensions` | 当前版本不支持那套 macOS 补丁。我们的包里有设置路由和 slug 属性，没有沿用他们的改包器 |
| DOM 设置后备 | 仓库有 `settings.js`，生产补丁不执行它 | 尚未验证他们的选择器 `nav.sidebar-navigation` 是否存在于这版 |
| 渲染扩展 | 已实现。主进程把函数字面量交给 `webContents.executeJavaScript` | 机制可参考。我们这次用的是追加 Preload，没有执行他们的运行时 |
| React 复用 | 已实现。从官方包里找出已经绑定的 React，再调用它的钩子和组件 | 要改官方包。这次没有做 |
| MutationObserver | 已实现，用于扩展自己的刷新和未进生产路径的设置后备 | 我们的宿主也观察 `documentElement`。选择器改用这版壳层包里的属性 |
| 窗口和 View | 已实现的是 `BrowserWindow` 与 `webContents`。仓库没有 `WebContentsView`、`BaseWindow`、`addChildView` | 不能把他们的窗口代码当成 Owl 视图方案 |
| 公开 SDK | 仓库没有 `@codex-plusplus/sdk`，也没有 `ui.registerSidebarItem` | 扩展接口要自己定。他们的 `api` 只有样式、事件和清理 |
| 路线图 | 没有 Roadmap 文件。README 把远程扩展目录写成以后的工作 | 这次也不做扩展市场 |
| Linux | 仓库没有 Linux 实现 | 当前版本不支持把该项目当作 Fedora 方案 |

他们的安装方式与这次的 Loader 同类，仓库源码里写的是前置，不是替换。  
`package.json` 的 `main` 必须是 `.vite/build/early-bootstrap.js`。  
补丁把一段 bootstrap 加在原文前面，ASAR 新内容追加到 payload 末尾，再重写 header 和完整性摘要。  
仓库没有自己的 `preload.js`，也不关 Fuse。  
这一段只说明机制相同，不能把 macOS 的签名和 `Info.plist` 写法搬到 Fedora。

他们的 IPC 允许列表也写在仓库里。  
处理消息前要求 `senderFrame === sender.mainFrame`。  
路径只接受 `/index.html`、`/detached-window.html`、`/settings` 和 `/settings/` 前缀，协议是 `app:`，host 是 `-`。  
我们目前只检查顶层帧和 `app:` 协议，路径列表还没加上。  
他们依赖的 `electronBridge` 和 `codex_desktop:` 通道是 ChatGPT 桌面端私有契约。  
这版 Owl 是否仍是这套，尚未验证，所以没有包装成 SDK。

## 原型现在由谁负责

**主进程拥有注册表，渲染宿主只负责把清单画出来。**  
扩展在 `setup` 里调用 `api.ui.registerSidebarItem`。  
[session-board](../src/extensions/session-board/extension.ts) 登记的标题是 `Session Board`，正文是 `Codex Enhancer UI integration successful.`  
[hello-world](../src/extensions/hello-world/extension.ts) 只写加载日志，不再加悬浮按钮。  
重复 id 抛出 `sidebar item id already registered`。  
返回的函数只删除同一扩展拥有的那条，主进程再把新清单发给 `app:` 页面。  
宿主发现清单里没有的按钮就移除。  
整份 POC 的卸载仍是原来的归档恢复，这次没有改 Loader 标记。

页面节点放在 `#root` 外面，避免 React 提交时把它删掉。  
正文用 `textContent` 写入。  
扩展代码碰不到 `ipcRenderer`，也碰不到 `WebContentsView`。  
Owl 的构造函数留在官方主进程里，没有收进 `api`。

目标形态可以收成这样。  
`route` 在官方路由器拥有注册口之前，只表示扩展自己的页面 id。

```javascript
export default {
  id: "session-board",
  name: "Session Board",
  setup(api) {
    const remove = api.ui.registerSidebarItem({
      id: "session-board",
      title: "Session Board",
      icon: "layout-dashboard",
      route: "/session-board",
      page: {
        title: "Session Board",
        body: "Codex Enhancer UI integration successful.",
      },
    });
    return remove;
  },
};
```

## 验收

| 验收项 | 这次的结果 | 证据档 |
|---|---|---|
| 原有侧边栏 | 登录页没有轨。登录按钮仍在 | 已运行 |
| 自定义入口 | 未出现。宿主在等 `nav[data-app-navigation-rail]` | 已运行 |
| 页面在主窗口内打开 | 未打开。没有新的 `BrowserWindow` 日志 | 已运行 |
| 返回原界面 | `Back` 和目的地点击会移除覆盖层 | 尚未验证 |
| 复用原生组件 | 只复制按钮 `className` 和工作区的计算样式 | 源码确认做不到组件导入 |
| 重新渲染后恢复入口 | 观察者会按清单补按钮 | 尚未验证 |
| 扩展卸载清掉入口 | 注册表删除有单测。画面清理要有轨才看得到 | 单测已运行，画面尚未验证 |
| IPC 来源校验 | 顶层 `app:` 通过，顶层 `codex-sandbox:` 拒绝 | 已运行 |
| 官方功能不受影响 | 登录页保持原样，没有浮层 | 已运行。登录后的聊天尚未验证 |
| 重启后注入仍在 | 16:23 的新进程再次进入 Loader 和 Preload | 已运行 |

## 以后会变的地方

壳层包的文件名带哈希。  
`data-app-navigation-rail` 和 `data-sidebar-destination` 这次是稳定属性，下一版仍可能改名或改成只在部分产品模式出现。  
目的地集合取决于账号和 gate。未登录时整条轨不挂载。  
设置项若要做成像官方目录里的一行，就要改设置包里的内部数组。那一步这次没有做，也不建议当成 SDK 的第一步。  
`WebContentsView` 的构造参数和挂载方式都还是 Owl 私有实现。调用点没定位完之前，不能写成稳定 API。  
IPC 还要补上路径允许列表，才能接近 Codex++ 那种主框架限制。  
观察 `document.documentElement` 的整棵子树，页面很大时会频繁回调。登录后如果卡顿，再把观察范围收进轨节点。
