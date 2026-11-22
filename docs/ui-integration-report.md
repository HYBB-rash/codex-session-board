# Codex Enhancer POC 0.2 汇报

证据分五档。  
**已运行**指测试副本里发生过。  
**源码确认**指从当前 `app.asar` 或 [Codex++ 2.1.0](https://github.com/b-nnett/codex-plusplus/blob/main/VERSION) 的仓库读到。  
**运行时推测**指日志或画面只给出方向。  
**尚未验证**指代码在，这次没有走到。  
**当前版本不支持**指这份安装或那个仓库没有这条能力。

环境是 Fedora 上的 Codex Desktop `26.1111.71704`，运行时 `owl`，Chromium `157.0.8103.78`。  
日常安装 `/opt/codex-desktop` 和日常配置没有改。  
测试副本用独立应用 ID、独立用户目录，启动注入仍是 0.1 的外部 Loader。  
Fuse 和 Chromium sandbox 保持打开，官方 Preload 没有被替换，官方业务包没有改。

## 结论

**这份桌面版能接入扩展界面的办法，就是 DOM 注入。**  
登录后的主窗口里，Session Board 以图标出现在官方导航轨上。  
点击后，页面盖住右侧主内容区，左侧会话栏和菜单仍在。  
关掉页面后，回到原来的聊天输入界面。  
没有新开窗口，也没有进入官方路由。

React 组件和官方路由没有给第三方的注册口。  
要复用那些组件，就得改官方业务包。  
Owl 能构造主渲染用的 `WebContentsView`，这项构造不能往导航轨里登记一项。  
这次没有调用它。

## 登录后的画面

**已运行：入口、页面和返回都在同一个 `app://-/index.html` 里完成。**  
[导航轨入口](screenshots/sidebar-entry.png) 里，官方首页和帮助仍在，Session Board 是帮助下面的四格图标。  
[主内容区页面](screenshots/session-board.png) 的标题是 `Session Board`，正文是 `Codex Enhancer UI integration successful.`  
[返回后的聊天页](screenshots/page-closed.png) 恢复了「我们要做什么？」和输入框。  
16:54 的 `page-rendered` 记下矩形左 340、上 36、宽 746、高 720，背景 `rgb(246, 246, 246)`。  
16:55 的 `page-closed` 之后，地址仍是 `app://-/index.html`。

导航轨很窄，可见文字会撑出轨外。  
所以轨内标题放在 `sr-only` 里，按钮保留 `aria-label` 和 `title`。  
图标是宿主自己画的四格，没有接到官方图标库。  
Back 目前是普通文字，还没有变成官方的胶囊按钮。

未登录时整条轨不存在。  
[登录页](screenshots/login-shell-absent.png) 上 `rail`、`destinations`、`workspace` 都是 0，入口不会提前画出来。

## 官方界面

**源码确认：功能导航和会话列表是两块不同的界面。**  
壳层包是 `webview/assets/async/14459.bfda23f585.js`。  
功能轨是 `nav[data-app-navigation-rail]`。  
无障碍名来自 `sidebarElectron.appNavigation`，中文是「应用导航」，默认英文是 `App navigation`。  
目的地按钮带 `data-sidebar-destination`，值来自目的地对象的 id。  
包里能读到 `builtin:home`、`builtin:codex`、`builtin:library`、`builtin:sites`、`builtin:orbit`。  
实际画出哪几项，还要经过产品模式条件。这次登录后的轨上看到的是首页、帮助和更多。  
会话行 `data-app-action-sidebar-thread-row` 在另一个包里，不属于这道轨。

**源码确认：目的地和设置项都是包内目录，没有扩展注册函数。**  
目的地的 `onSelect` 走应用内部导航。  
设置路由在模块 `OSCq`，路径是 `/settings`，并带 `settingsLayout: true`。  
`settingsNavigationSections` 由设置包里的内部函数返回。  
读到的一处设置项把 `data-settings-panel-slug` 写成字面量 `teams`。  
没有读到扩展可以调用的注册导出。

**源码确认：主页面不能把外部文件嵌进 iframe。**  
`webview/index.html` 的 `default-src` 是 `'none'`。  
`script-src` 只有 `'self'`、一段固定哈希和列出的外部脚本。  
`frame-src` 没有 `file:`。  
`style-src` 允许 `'unsafe-inline'`。  
因此页面由 Preload 在当前文档里建节点。

**源码确认：Owl 的视图接口停留在私有构造。**  
`createAppView` 用 `getRendererPreferences("primary", path)` 构造 `WebContentsView`。  
同一文件能读到 `contentView.children` 里有这种视图。  
该文件没有 `addChildView`、`setContentView`、`BaseWindow`。  
`createAppView` 挂到窗口上的那一行，这次没有定位到。

## 三种方案

| 方案 | 现状 | 依据 |
|---|---|---|
| DOM 注入 | 已采用，登录后已在主窗口跑通 | 已运行。选择器来自壳层包里的属性 |
| React 组件复用 | 当前版本不支持当作扩展接口 | 源码确认：组件在哈希包里，类名也是哈希，路由是内部目录 |
| Owl View | 当前版本不支持当作侧边栏注册 | 源码确认：有 `WebContentsView` 构造，没有侧边栏注册。挂载行未定位 |

**已决定：扩展页面继续走 DOM 注入，不改官方业务包。**  
宿主等 `DOMContentLoaded` 后观察文档。  
按钮挂在轨内最后一个 `data-sidebar-destination` 后面，并复制该按钮的 `className`。  
页面是 `document.body` 上的 `section`，位置取 `data-app-shell-workspace`，再减去导航轨、页面侧栏和 `[data-app-action-sidebar-scroll]` 的右缘。  
宽度超过工作区 70% 的节点不当作侧栏，避免把整块工作区减没。  
背景向上找到第一个不透明颜色。  
`route` 只存在注册表里。  
React 把按钮清掉之后，观察者按当前清单补回。  
启动画面换成壳层时，这次已经补上过一次。登录之后的中途整页重绘，还没有单独再测。

## Codex++

材料来自该仓库 2026-10-15 的 2.1.0 源码。  
`Package.swift` 只声明 macOS 14。  
钉死的适配器是 ChatGPT `26.1007.21159`（build 20052）和 `26.1002.52244`（build 13536）。  
README 另写审核过 ChatGPT `26.1007.71326`，build 20398，arm64。  
这些都不是本机的 `26.1111.71704`。  
本机没有运行它的安装器。

| 题目 | 仓库里的状态 | 对这份 Fedora 构建的意义 |
|---|---|---|
| 设置导航 | 已实现。用 Acorn 改官方设置模块，插入 slug `codexpp-extensions` | 当前版本不支持那套 macOS 补丁。那是在改官方包 |
| 渲染扩展 | 已实现。主进程把函数字面量交给 `webContents.executeJavaScript` | 可以参考注入边界。我们用的是追加 Preload |
| React 复用 | 已实现。从官方包里找出已经绑定的 React 再调用 | 同样要改官方包。这次没有做 |
| 窗口和 View | 仓库没有 `WebContentsView`、`BaseWindow`、`addChildView`，也没有 Owl 这个名字 | 不能把它当成 Owl 视图方案 |
| 公开 SDK | 没有 `@codex-plusplus/sdk`，也没有 `ui.registerSidebarItem` | 扩展接口要自己定 |
| Linux | 仓库没有 Linux 实现 | 当前版本不支持把该项目当作 Fedora 方案 |

他们的安装也是在 `.vite/build/early-bootstrap.js` 前面加代码，并保留官方 Preload。  
IPC 要求 `senderFrame === sender.mainFrame`，路径只接受 `/index.html`、`/detached-window.html`、`/settings` 和 `/settings/` 前缀。  
我们目前只检查顶层帧和 `app:` 协议。  
他们依赖的 `electronBridge` 和 `codex_desktop:` 通道，这版 Owl 是否仍提供，尚未验证，所以没有收进接口。

## 扩展接口

**主进程拥有注册表，渲染宿主只负责把清单画出来。**  
扩展在 `setup` 里调用 `api.ui.registerSidebarItem`，碰不到 `ipcRenderer`，也碰不到 `WebContentsView`。  
重复 id 抛出 `sidebar item id already registered`。  
返回的函数只删除同一扩展拥有的那一条，主进程再把新清单发给顶层 `app:` 页面。  
宿主发现清单里没有的按钮就移除。  
整份 POC 的卸载仍是原来的归档恢复。扩展级的画面卸载有单测，登录后的窗口上还没有点过。

已运行的 IPC 只接受 `parent === null` 且协议为 `app:` 的帧。  
顶层 `codex-sandbox:` 可视化页被拒绝。  
缺 `parent` 的帧按单测拒绝。  
路径还没有收成 `/index.html`。  
沙箱 Preload 不能加载旁边的文件，所以 `runtime/preload.js` 在构建时已经包含宿主。

```javascript
export default {
  id: "session-board",
  name: "Session Board",
  setup(api) {
    return api.ui.registerSidebarItem({
      id: "session-board",
      title: "Session Board",
      icon: "layout-dashboard",
      route: "/session-board",
      page: {
        title: "Session Board",
        body: "Codex Enhancer UI integration successful.",
      },
    });
  },
};
```

`icon` 和 `route` 都会保存。  
在官方路由器和图标库出现可调用的注册口之前，这两个字段不驱动官方界面。

## 验收

| 验收项 | 结果 | 证据档 |
|---|---|---|
| 原有侧边栏 | 登录后导航轨和会话栏都在 | 已运行 |
| 自定义入口 | 四格图标出现在官方导航轨里 | 已运行 |
| 页面在主窗口内打开 | 盖住主内容区，左 340、宽 746 | 已运行 |
| 返回原界面 | 覆盖层消失，聊天输入框回来 | 已运行 |
| 原生组件 | 复制了轨按钮的 class。页面是自绘 DOM | 已运行 |
| 页面重绘后恢复 | 启动画面换成壳层后按钮出现 | 已运行。中途整页重绘尚未单独再测 |
| 扩展卸载 | 注册表删除有单测 | 画面尚未验证 |
| IPC 来源 | 顶层 `app:` 通过，顶层 `codex-sandbox:` 拒绝 | 已运行 |
| 官方功能 | 返回后聊天页、新聊天和项目选择都在 | 已运行 |
| 重启 | 登录会话保留，新进程再次挂上入口 | 已运行 |

## 后续会碰到的限制

壳层包的文件名带哈希。  
`data-app-navigation-rail` 和 `data-sidebar-destination` 这次能用，下一版仍可能改名，或只在部分产品模式出现。  
未登录时轨不挂载，入口无从插入。  
设置项若要变成官方目录里的一行，就要改设置包里的内部数组。  
`WebContentsView` 的构造和挂载都还是 Owl 私有实现。  
IPC 还要补上路径允许列表。  
观察整棵文档在大页面上会频繁回调。若登录后感到卡顿，再把观察范围收进轨节点。
