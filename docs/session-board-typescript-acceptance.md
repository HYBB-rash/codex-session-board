# TypeScript 会话看板验收

上游行为基线为 `2cc4e6bb4e03d7cfabce0733a91a328004a9f3fc`。Fedora 44 的 Enhancer 副本及真实 RPM 原地安装均通过本机和远程原生移卡。macOS arm64 的官方 Codex `26.1111.21308` 已通过自动副本安装、重启、卸载、本机移卡及从 Mac 发起的 Fedora 原生移卡。完整发布门槛仍未达到。

## 0.3.31 候选

2026 年 11 月 23 日从提交 `ec0f152` 构建。看板回执记录进程启动身份，旧版回执也能识别已由无关进程接手的 PID。从干净解包运行门槛，两端归档校验和 78 项自动检查通过，31 条冻结需求中 30 条达到本版双平台门槛。macOS 通过 40/40 条现场断言，Fedora 通过 39/40 条，剩余 `APP-001` 的最左侧原生固定图标直达。 [本版门槛](../state/acceptance/release-gate-0.3.31.json) · [本版现场证据](../state/acceptance/live-0.3.31.json)

| 场景 | 现场观察 |
| --- | --- |
| Fedora 解包 | 102 项 Node 测试、12 项公共入口与浏览器测试、冻结模型和 18 项门槛测试通过。 [随包测试](../state/acceptance/fedora-archive-0.3.31-final.log) |
| Fedora 原地 | 从隔离环境的真实 RPM 原图标启动，0.3.31 识别并替换指向无关存活进程的旧回执。无关进程继续运行，本机原生连接及专用卡读回成功。另从 0.3.30 原地更新、重启、回退、再重启，专用卡分区和备注摘要一致，卸载后 RPM 文件、启动器与桌面项验回。 [回执恢复](../state/acceptance/in-place-rootless/signed-receipt-recovery-0.3.31.txt) · [原地更新和回退](../state/acceptance/in-place-rootless/signed-release-transition-0.3.31.txt) |
| Fedora 原生双端移卡 | 从 RPM 原图标启动，经插件详情打开原生 MCP App，核对随包脚本及回环网页状态。本机和 MacBook Pro 的专用卡均实际跨列，原生分区独立读回，刷新保留备注，反拖恢复。远端卡复验同列、拖出及横滚。卸载后核对 RPM 原件。 [入口与登录](../state/acceptance/in-place-rootless/signed-native-mcp-click-0.3.31.jsonl) · [双端拖放和复位](../state/acceptance/in-place-rootless/signed-native-mcp-drag-retry-0.3.31.jsonl) · [卸载](../state/acceptance/in-place-rootless/signed-final-uninstall-0.3.31.txt) |
| Fedora 图形验收 CLI | 在独立 Xvfb、D-Bus 和运行目录中用 `--ozone-platform=x11` 启动已有副本。原生接口确认 ChatGPT 登录，本机和 MacBook Pro 通过 Codex 已有连接读回。两端专用卡均用可信指针跨列，独立原生读回与侧栏一致，反拖后分区和备注验回。退出测试窗口，回退副本版本选择至 0.3.24，三份受保护文件的摘要一致。 [登录、双端拖放和退出](../state/acceptance/fedora-0.3.31-gui-cli-native-two-hosts.log) |
| Fedora 公共入口与 PWA | 副本重启读回 0.3.31。HTTP、CLI 与 MCP 在同一本机专用卡上读回一致状态和回复，MCP 枚举 18 项共享操作。隔离浏览器安装的系统启动项两次打开独立看板窗口。部分读取时界面显示异常，缓存只读又保留已保存的版本及备注，恢复配置后重扫成功。复验后副本回退到 0.3.24。 [公共入口](../state/acceptance/fedora-0.3.31-entrypoints-local.log) · [PWA](../state/acceptance/fedora-pwa-installed-0.3.31.json) · [部分读取](../state/acceptance/fedora-0.3.31-health-local.log) · [副本回退](../state/acceptance/fedora-signed-copy-rollback-0.3.31-field-finish-local-field.jsonl) |
| Fedora 发现 | 每页一条读完本机和 MacBook Pro，两端独立原生枚举合计 12 条稳定 ID，与看板一致。恢复配置后再次核对。断网的真实 App Server 补验三种交互来源、两个提供方、子代理排除和分页上限。 [原生分页](../state/acceptance/fedora-0.3.31-discovery-two-hosts.log) · [来源夹具](../state/acceptance/fedora-discovery-sources-0.3.31-offline.json) |
| Fedora 补充现场检查 | 六列顺序、导航和表单选项均与冻结规范一致。在六列模式下利用既有本机分区完成三次跨列并复位，未修改原生分区目录。标题保存和重启重试、同名导航和预览、阻塞表单、筛选键盘交互及滚动均补验通过。 [六列跨列](../state/acceptance/fedora-native-six-columns-0.3.31-1795435556078.json) · [标题重试](../state/acceptance/fedora-title-retry-0.3.31-ccd3cddff.jsonl) · [标题预算](../state/acceptance/fedora-title-003-ack-0.3.31-9f0f5b64f.jsonl) · [导航](../state/acceptance/same-title-0.3.31-1f8774fc-e160-4737-a67d-3581aa4ca3ef.json) · [表单](../state/acceptance/fedora-ui-002-0.3.31-1795434353862.jsonl) · [滚动及键盘](../state/acceptance/fedora-ui-003-0.3.31-1795434515272.jsonl) |
| Fedora 原地运行保护 | 原图标启动并读回专用卡后，安装、更新、回退和卸载四个命令均因进程存活而拒绝。原地回执及 RPM 文件保持，退出后卸载验回原件。 [实际进程保护及卸载](../state/acceptance/in-place-rootless/signed-live-process-guard-0.3.31.txt) |
| 双平台原生轮次 | 两端均观察到批准等待、可信批准后的同轮执行、暂时错误重试期间保持进行中，以及澄清结束后读回待审阅和完整答复。测试卡已归档。Mac 脚本内的配置差异检查中止了清理记录，外层恢复了配置和专用副本，随后独立核对归档索引、归档文件和恢复后的写范围。 [Fedora 轮次](../state/acceptance/desktop-approval-0.3.31-007f9387.jsonl) · [Mac 轮次](../state/acceptance/packages/macos-0.3.31-native-turns.log) · [Mac 清理核验](../state/acceptance/packages/macos-0.3.31-native-turns-cleanup.json) |
| MacBook Pro 验收 CLI | 通过已建立的 SSH 控制连接和 Homebrew Python 在真机重放。CLI 校验并投送本版归档，创建干净暂存，验回原生轮次、归档、两份配置逐字节恢复及原测试副本恢复。首次运行缺少隔离看板配置，补齐初始化并限定所选原生窗口后，复跑通过。 [真机 CLI 原始日志](../state/acceptance/cli-runs/46db85a7cecc4b2a/run.log) · [结果](../state/acceptance/cli-runs/46db85a7cecc4b2a/status.json) |
| MacBook Pro 解包 | 真机核对归档 SHA-256，99 项 Node 测试、12 项公共入口与浏览器测试、冻结模型和 18 项门槛测试通过。 [包指纹及 Node 测试](../state/acceptance/packages/macos-0.3.31-package-tests.log) · [随包门槛](../state/acceptance/packages/macos-0.3.31-package-gates.log) |
| MacBook Pro 原生 MCP App | 从专用市场注册后经插件详情打开，核对随包脚本及备用网页状态。可信跨列、反拖、刷新、同列、拖出及边缘横滚均通过。六列名称、顺序和表单选项一致，六列下原生跨列和反拖验回。标题拖动保持路由，标题与相邻调整控件分别执行各自操作。筛选保留空列，键盘取消和保存、操作期间暂停重绘、刷新保留滚动均通过。阻塞表单验回空备注、取消、旧版本拒绝及空草稿。 [拖放、筛选和标题控件](../state/acceptance/packages/macos-0.3.31-native-mcp-extended-final.log) · [六列复验](../state/acceptance/packages/macos-0.3.31-native-mcp-legacy.log) · [阻塞表单及复原](../state/acceptance/packages/macos-0.3.31-native-mcp-extended-bounded.log) |
| MacBook Pro 最左侧原生入口 | 账号已有这项固定。离开看板后，可信点击原生应用栏的固定图标，直接打开随包看板。专用卡的状态和备注与备用网页一致，加载的看板脚本与发行包摘要一致。只读核对账号原值和点击后的设置，两次摘要相同。候选已卸载，此前的专用副本已恢复。 [原生直达与清理](../state/acceptance/macos-0.3.31-native-sidebar-direct-cache-verified.jsonl) · [账号设置原值](../state/acceptance/macos-0.3.31-native-sidebar-preflight.jsonl) · [独立读回](../state/acceptance/macos-0.3.31-native-sidebar-direct.jsonl) |
| MacBook Pro 发现与部分读取 | 已安装副本沿用真实本机连接，逐页独立读回两张专用卡的稳定 ID。一页上限触发部分读取和观察未就绪提示，已保存状态、备注和版本保持，恢复配置后扫描正常。临时恢复的一张测试卡已重新归档。断网的官方 App Server 同版夹具另外读回三页，覆盖三种交互来源、两个提供方及子代理排除。 [真实窗口与恢复](../state/acceptance/packages/macos-0.3.31-gui-cli-health-partial.log) · [断网 App Server 回执](../state/acceptance/packages/macos-discovery-sources-0.3.31-offline.json) |
| MacBook Pro 标题同步 | 在已登录专用副本建立命名夹具。独立原生改名后，从看板保存状态，同次保存写入当前状态方括号前缀，原生读回完整正文、表情和其他标签。复位后原生标题、分区及备注一致，测试卡归档，两份配置逐字节恢复。 [同次保存与清理](../state/acceptance/packages/macos-0.3.31-gui-cli-title-manual.log) |
| MacBook Pro 标题重试 | 专用库拒绝改名后，原生分区改变，备注、保存版本和标题待办同时保留。重新启动后解除拒绝，后台改名并清空待办。在同一运行构建中暂扣原生改名回执，RPC 预算为 2935 毫秒，较早回执仍被暂扣时前台保存用时 153 毫秒，迟到回执未清除较新待办。测试卡复位归档，故障夹具已移除。 [重启及重试](../state/acceptance/packages/macos-0.3.31-gui-cli-title-retry.log) · [预算、并发及清理](../state/acceptance/packages/macos-0.3.31-gui-cli-title-retry-budget.log) |
| MacBook Pro 预览和同名跳转 | 真实轮次进行中从窗口读回上一条完整回复。两张同名专用卡各自预览完整回复并进入不同的原生会话 ID，会话集合、轮次和卡片版本保持。随包浏览器夹具复验迟到回复、空回复和读取失败。两张专用卡已归档，两份配置逐字节恢复。 [真实轮次及跳转](../state/acceptance/packages/macos-0.3.31-gui-cli-native-navigation-categorized.log) · [随包浏览器夹具](../state/acceptance/packages/macos-0.3.31-gui-cli-preview-fixture.log) |
| MacBook Pro 图形验收 CLI | 通过同一 CLI 在已登录的 Mac 图形会话启动专用副本。原生 MCP App 的可信跨列、反拖、刷新及备注保存复验通过。另一次启动经原生接口确认 ChatGPT 登录，已有 Fedora 连接完成扫描和运行观察，独立原生列表返回会话。各次候选均卸载并恢复此前专用副本。 [本机拖放](../state/acceptance/packages/macos-0.3.31-gui-cli-native-drag.log) · [原生登录及 Fedora 列表](../state/acceptance/packages/macos-0.3.31-gui-cli-account-health.log) |
| MacBook Pro 发起远端移卡 | 2026 年 11 月 24 日从新启动的 `0.3.31` 专用窗口沿用既有 Fedora 主机登记。从看板拖入进行中，原生侧栏同步显示，Fedora 独立读回目标分区。补验从 Codex 原生侧栏移回同一张卡，旧表单被拒绝并保留草稿，已保存备注和标题保持。专用卡归档并经独立列表确认，候选卸载后验回看板配置、连接登记和自动连接偏好。 [完整移卡检查点](session-board-native-checkpoint.md#macbook-pro-桌面实测) · [原生侧栏移回和旧表单](../state/acceptance/packages/macos-0.3.31-remote-sidebar-drag-and-stale-20261124.jsonl) · [SSH 配置及服务端身份核对](../state/acceptance/fedora-0.3.31-mac-ssh-roundtrip-preserved-20261124.json) |
| macOS 签名连续性 | 在官方 Codex `26.1111.21308` 上，`0.3.31` 的 ad hoc 副本通过严格验签，以原包的指定签名要求复核时被拒绝。卸载后副本重新满足该要求，启动归档、主程序、`Info.plist` 和签名资源清单均验回原件。本版 `--in-place` 在 macOS 上由预检拒绝，官方包签名和四个文件的摘要验回。 [本版验签和复原回执](../state/acceptance/packages/macos-0.3.31-signature-continuity-verified.jsonl) · [原地预检](../state/acceptance/packages/macos-0.3.31-in-place-preflight-exact.json) |
| macOS 官方签名对照 | 从原样复制并通过官方签名核验的客户端启动。它沿用专用登录，原生接口能读同一张本机专用卡。只登记 `0.3.31` MCP 插件后，最左侧原生图标能打开随包界面。看板提示连接断开，显示零张卡。对空状态目录直接调用随包 `board_snapshot` 也提示启动带 Enhancer 的窗口。退出后恢复原测试配置和此前的专用副本，并复核系统应用签名和文件。 [实际窗口与清理](../state/acceptance/packages/macos-0.3.31-stock-native-plugin-final.jsonl) · [随包 MCP 对照](../state/acceptance/packages/macos-0.3.31-mcp-exact-no-runtime.json) |
| macOS 官方窗口的 App Server 通道 | 从保持官方签名的副本按默认通道启动。客户端日志记录 `stdio`，所属 App Server 无命名 Unix 套接字和 TCP 监听。App Tools 使用另有客户端授权的套接字。官方包中本机守护进程和 WebSocket 均需显式选用，不能接入这次已启动的窗口。此次渲染器未重新验回登录，以前一条对照记录为准。已恢复专用配置和此前窗口，系统应用签名及文件摘要验回。 [通道现场回执](../state/acceptance/packages/macos-0.3.31-stock-app-server-live-transport.jsonl) · [官方包通道实现](../state/acceptance/packages/macos-0.3.31-stock-transport-contract.json) |

2026 年 11 月 23 日 Mac 发起的 Fedora 连接曾在 SSH 签名阶段超时。11 月 24 日，用户授权为该既有登记配置专用密钥及 Apple Keychain。本轮核对了无 Agent、无控制连接复用的新认证和服务端接受的身份。Mac 整机睡眠和重启仍未实测。 [此前签名阶段](../state/acceptance/packages/macos-0.3.31-native-route-final-recheck.json) · [新认证](../state/acceptance/macos-0.3.31-dedicated-ssh-live-auth-20261124.json)

Fedora 原装客户端的固定按钮在 ChatGPT 登录模式下写入账号侧栏设置。两台专用副本的原生登录身份不同。Fedora 隔离 RPM 已登录并能从插件详情打开看板，离开看板后，最左侧原生应用栏未显示该图标。该次验收没有写入账号设置，已退出隔离窗口并验回 RPM 和测试配置。 [固定入口写入范围](../state/acceptance/fedora-0.3.31-native-pin-auth-boundary.json) · [账号身份对比](../state/acceptance/fedora-0.3.31-native-sidebar-account-scope.json) · [隔离 RPM 只读回执](../state/acceptance/fedora-0.3.31-native-sidebar-rpm-read-only.jsonl)

从 Fedora 专用副本再次读取账号设置，服务端返回成功，客户端已登记候选，但没有返回侧栏自定义对象。原生「重置自定义」会写入空的固定覆盖和当前入口顺序。该操作能清除自定义固定，但尚无证据证明能恢复云端原先没有自定义对象的状态。 [账号设置只读回执](../state/acceptance/fedora-0.3.31-native-sidebar-baseline-read-only.jsonl) · [原生重置实现核对](../state/acceptance/fedora-0.3.31-native-sidebar-default-reset-source.json)

- [Fedora x86_64 安装候选](../state/acceptance/packages/session-board-0.3.31-linux-x86_64.tar.gz "sha256=3fce27047e049ae90a2ca421161841320c3cd6754d9ee929207880b5990c684c") · [校验文件](../state/acceptance/packages/session-board-0.3.31-linux-x86_64.tar.gz.sha256)
- [macOS arm64 安装候选](../state/acceptance/packages/session-board-0.3.31-macos-arm64.tar.gz "sha256=8d5d309aade6f3d801a45ac85306c83274ac5c267637c220499796e55ea1fc08") · [校验文件](../state/acceptance/packages/session-board-0.3.31-macos-arm64.tar.gz.sha256)
- [逐项验收证据归档](../state/acceptance/packages/session-board-0.3.31-acceptance.tar.gz "sha256=bcf0a22bf49dbe63bf9b27fe64b3ccc943c3774ff1714942d778820d11e8ed90") · [校验文件](../state/acceptance/packages/session-board-0.3.31-acceptance.tar.gz.sha256)

证据归档自带 44 份由现场索引引用的回执、额外的平台边界证据和两个安装包的摘要。从干净解包运行随包检查，复现同一份逐项门槛报告。

## 0.3.30 候选

2026 年 11 月 23 日从提交 `2137b7e` 构建。跟踪的新建请求先核对项目所属主机，再登记或重放。双平台归档内容及 SHA-256 校验通过，78 项门槛自动检查通过。40 条现场断言中，Fedora 通过 39 条，macOS 通过 10 条。31 条冻结需求中 21 条达到本版双平台门槛。Fedora 剩余最左侧原生固定图标直达。 [新建回归](../src/extensions/session-board/project-assignment.test.ts) · [本版门槛](../state/acceptance/release-gate-0.3.30.json) · [本版双平台证据索引](../state/acceptance/live-0.3.30.json)

| 场景 | 现场观察 |
| --- | --- |
| Fedora 解包 | 101 项 Node 测试通过，跳过 1 项 macOS 专用测试。12 项公共入口和浏览器测试、冻结模型及 18 项门槛测试通过。 [随包测试](../state/acceptance/fedora-archive-0.3.30-final.log) |
| Fedora 副本及本机输入页 | 从已安装看板点击更新，重启读回 `0.3.30`。请求把远端项目指向本机时返回 HTTP 400，账本不变。默认本机入口打开一处空白原生输入区，独立枚举的活动和归档 ID 集合一致，未收到会话或轮次启动事件。回退并重启后读回 `0.3.24`，专用卡、系统归档和隔离配置校验一致。 [真实更新](../state/acceptance/fedora-signed-copy-update-0.3.30.jsonl) · [原生输入区](../state/acceptance/fedora-native-composer-0.3.30.json) · [回退](../state/acceptance/fedora-signed-copy-rollback-0.3.30-local-update.jsonl) |
| Fedora 原地与原生 MCP App | 隔离 Fedora 从原 RPM 桌面入口启动，验回已有登录和随包原生界面。插件详情打开的 MCP App 与备用网页读回同一本机卡。可信拖放跨列、刷新和反拖后均独立核对原生分区和非空备注。同列及拖出无保存，右缘横滚到 396 像素的最右端。卸载复核原启动器、桌面项和 RPM 文件。此次仅验证本机，最左侧原生固定图标仍待专用账号复验。 [原地安装](../state/acceptance/in-place-rootless/signed-reinstall-0.3.30.txt) · [界面和登录](../state/acceptance/in-place-rootless/signed-native-mcp-click-0.3.30.jsonl) · [原生跨列](../state/acceptance/in-place-rootless/signed-native-mcp-drag-retry-0.3.30.jsonl) · [卸载](../state/acceptance/in-place-rootless/signed-final-uninstall-0.3.30.txt) |
| Fedora RPM 事务与恢复 | 无网卡、SELinux enforcing 的 Fedora 客体复验安装写入失败及进程中断，原文件及元数据保持，重装和卸载均成功。同版 RPM 在无图形用户总线时替换 Loader，下一次登录由用户服务提示，卸载保留 RPM。另用改名入口的 RPM 夹具复验事务锁、回执丢失、替换识别及卸载后原件保留。原版启动探针在该客体数次未等到原生窗口就绪，运行中卸载保护及原地新运行时切换待本版复验。 [中断恢复](../state/acceptance/in-place-rpm/results/enhancer-030-recover.txt) · [登录后提示](../state/acceptance/in-place-rpm/results/enhancer-030-late-only-late.txt) · [改名入口](../state/acceptance/in-place-rpm/results/enhancer-030-rename-only-rename.txt) · [客体启动探针](../state/acceptance/in-place-rpm/results/original-launch-final-030-cold-stock-guard.txt) |
| Fedora 发现与导航 | 每页一条完整枚举本机，独立原生 ID 集合一致。限制为一页后界面提示部分读取，缓存只读，恢复配置后扫描成功。备用网页在原生远端离线时保留异常提示。断网真实 App Server 补验三种会话来源及两个提供方。两张同名专用卡的最近完整回复均与原生结果一致，分别跳转到原会话，期间无新建或发送。 [分页](../state/acceptance/fedora-0.3.30-discovery-local.log) · [诊断](../state/acceptance/fedora-0.3.30-health-local.log) · [远端离线](../state/acceptance/fedora-0.3.30-offline-health.log) · [来源](../state/acceptance/fedora-discovery-sources-0.3.30-offline.json) · [原生导航](../state/acceptance/same-title-0.3.30-fd749e18-a72c-4258-b5ca-c48941ee4510.json) |
| Fedora 轮次、表单及标题 | 原生批准等待、批准后同轮恢复、暂时错误重试、澄清结束和可信停止均有本版回执。标题拖动与相邻控件互不干扰，阻塞表单保留显式空值并拒绝三种旧版本。标题失败后重启读回同一待同步项，解锁后成功重试。原生改名预算耗尽后，迟到回执未清除较新任务。 [批准和澄清](../state/acceptance/desktop-approval-0.3.30-6c902425.jsonl) · [原生停止](../state/acceptance/same-title-0.3.30-fd749e18-a72c-4258-b5ca-c48941ee4510.json) · [阻塞表单](../state/acceptance/fedora-ui-002-0.3.30-1795426008105.jsonl) · [标题重试](../state/acceptance/fedora-title-retry-0.3.30-3c350c91b.jsonl) · [写入预算](../state/acceptance/fedora-title-003-ack-0.3.30-4d357e7ac.jsonl) |
| Fedora 六状态、键盘及滚动 | 独立读回七次原生移动及专用卡复位，两个临时分区已移除。筛选保留空列，可信拖放和键盘编辑跨过定时刷新未重绘，保存后读回原生分区。临时缩短真实待审阅列后，手动和周期刷新都保留 115 像素滚动位置及同一卡片锚点。 [六状态回执](../state/acceptance/fedora-native-six-columns-0.3.30-1795426492947.json) · [键盘与滚动](../state/acceptance/fedora-ui-003-0.3.30-1795426501369.jsonl) · [副本回退](../state/acceptance/fedora-signed-copy-rollback-0.3.30-field-finish-local-field-retry.jsonl) |
| Fedora 归档与恢复 | 筛选后归档本机专用卡，独立原生读回，关页重开保留批次。关闭看板时，定时任务仍归档专用卡，停用后清除截止时间。重启后核对隔离账本，模拟离线项继续待核对，本机已提交项独立确认。测试后取消归档，原生位置、标题、备注和配置验回。 [筛选归档](../state/acceptance/fedora-archive-batch-0.3.30-1795426731797.jsonl) · [关页定时归档](../state/acceptance/fedora-archive-batch-0.3.30-1795426735738.jsonl) · [重启核对](../state/acceptance/fedora-archive-reopened-checked-0.3.30.json) |
| Fedora PWA 与迁移 | 从隔离 Chrome 安装生成的系统启动项，两次打开独立窗口读取当前候选。用随包数据迁移入口验回主库和两个合成远端分库的已提交 WAL，离线记录保留，拒绝覆盖，回退后停止投影旧数据。 [系统 PWA](../state/acceptance/fedora-pwa-installed-0.3.30.json) · [迁移和回退](../state/acceptance/fedora-migration-0.3.30.json) |
| Fedora 账号切换界面 | 用经校验的随包脚本、两个模拟账号和隔离 Chrome 通过十项检查。换号清理旧卡、编辑器、回复和归档，迟到响应保持隐藏，返回后只恢复本账号草稿。 [模拟账号浏览器回执](../state/acceptance/browser-account-switch-0.3.30.json) |
| MacBook Pro 解包 | macOS arm64 真机复核归档 SHA-256，启动场景所用官方 Codex 构建为 `26.1111.21308`。随包 98 项 Node 测试、12 项公共入口与浏览器测试、冻结模型和 18 项门槛测试通过。 [真机身份](../state/acceptance/packages/macos-0.3.30-host-identity.txt) · [随包测试](../state/acceptance/packages/macos-0.3.30-native-turns.log) |
| MacBook Pro 副本与原生轮次 | 从 0.3.30 安装独立签名副本，复用专用已登录配置。原生批准等待进入已阻塞，点击允许一次后同一轮确实执行。暂时请求错误宣布重试时保持进行中。澄清答复结束后独立读回待审阅和原生回复。专用卡归档、两份配置逐字节恢复，候选卸载并恢复此前测试副本。 [原生轮次及清理](../state/acceptance/packages/macos-0.3.30-native-turns-fresh-profile.log) |

Fedora 原地切换探针在启用新版前失败。原进程残留回执指向已分配给无关进程的 PID，看板误报已有实例。隔离包复现表明，无关进程退出后使用同一回执即可启动。Mac 原生 MCP 补验在专用市场登记超时，候选已卸载并恢复测试副本。 [原地切换及恢复](../state/acceptance/in-place-rootless/signed-release-transition-0.3.30.txt) · [回执缺陷复现](../state/acceptance/fedora-0.3.30-stale-service-receipt-reproduction.json) · [Mac 市场登记](../state/acceptance/packages/macos-0.3.30-native-product-retry.log)

- [Fedora x86_64 安装候选](../state/acceptance/packages/session-board-0.3.30-linux-x86_64.tar.gz "sha256=0907fb8e8fed69895c44893152b6d41017a68671120f4dc2d92491c6e343f238") · [校验文件](../state/acceptance/packages/session-board-0.3.30-linux-x86_64.tar.gz.sha256)
- [macOS arm64 安装候选](../state/acceptance/packages/session-board-0.3.30-macos-arm64.tar.gz "sha256=f2e1c8a85f65b48e553386eafcf23f3e7153681a7965491b75b3fabec3406ec2") · [校验文件](../state/acceptance/packages/session-board-0.3.30-macos-arm64.tar.gz.sha256)

## 0.3.29 候选

2026 年 11 月 23 日从提交 `ca4bad9` 构建。31 条冻结需求中 21 条通过双平台门槛，10 条待验，78 项自动检查通过。40 条现场断言中，Fedora 通过 39 条，剩余原生最左侧固定图标直达。macOS 通过 16 条。`UI-001`、`UI-002` 和 `UI-003` 通过完整门槛。 [逐项门槛](../state/acceptance/release-gate-0.3.29.json) · [现场证据索引](../state/acceptance/live-0.3.29.json)

| 场景 | 现场观察 |
| --- | --- |
| 双平台干净解包 | Fedora 通过 101 项 Node 测试，跳过 1 项 macOS 专用测试。MacBook Pro 通过 98 项，跳过 4 项 Linux 专用测试。两端各通过 12 项公共入口及浏览器测试、10 项冻结模型检查和 18 项门槛测试。 [Fedora](../state/acceptance/fedora-archive-0.3.29-final.log) · [MacBook Pro](../state/acceptance/packages/macos-0.3.29-package-final.log) |
| Fedora 原地与原生 MCP App | 从真实 RPM 启动入口进入 Codex，经插件详情打开对应的原生界面。本机和 MacBook Pro 专用卡跨列后独立读回，刷新保留非空备注，反拖恢复原分区。同列、拖出和边缘横滚通过。卸载后复核 RPM、原启动器和桌面项。 [安装和拖放](../state/acceptance/fedora-0.3.29-in-place.log) · [卸载](../state/acceptance/in-place-rootless/signed-final-uninstall-0.3.29.txt) |
| Fedora 副本 | 在专用 Enhancer 窗口点击更新，重启后读回 `0.3.29`。复验标题拖动与原生路由、阻塞表单、搜索与键盘，以及系统 PWA 两次独立启动。回退到原运行版后复核两端专用卡、备注、标题和测试配置。 [更新](../state/acceptance/fedora-signed-copy-update-0.3.29.jsonl) · [交互复验](../state/acceptance/fedora-0.3.29-update-field.jsonl) · [回退](../state/acceptance/fedora-signed-copy-rollback-0.3.29.jsonl) |
| Fedora 标题重试 | 专用库拒绝改名时，状态、备注和待重试项已一起写入。重启后从窗口读到同版本待重试项，独立验回原生目标分区。解除拒绝后后台自动同步并读回标题，磁盘队列清空。专用卡和配置都已恢复。 [重启、重试和复位](../state/acceptance/fedora-title-retry-0.3.29-90aa4aa57.jsonl) |
| Fedora 本机补验 | 已装 `0.3.29` 的隔离窗口读回六个原生分区的实际移动，七次移动后恢复原标题、备注、原生位置和临时分区目录。真实轮次的原生批准等待、可信批准后的同轮实际执行、暂时错误自动重试均通过。本机原生标题在同次保存中读回。后台标题回执被暂扣时，前台保存用时 122.04 毫秒，改名 RPC 预算为 2,938 毫秒，迟到回执保持新待办。 [六状态](../state/acceptance/fedora-native-six-columns-0.3.29-1795420039940.json) · [批准和重试](../state/acceptance/fedora-0.3.29-extended-local.jsonl) · [标题复验](../state/acceptance/fedora-0.3.29-local-followup.jsonl) |
| Fedora 原生停止 | 隔离轮次进入进行中后，通过原生输入区的停止按钮中断同一轮。独立读回 `interrupted`，看板进入已阻塞，显示中断原因并保留备注，原生分区与看板一致。专用卡已归档，隔离配置及副本版本已恢复。上次补验曾在点击前遇到轮次列表接口错误，专用卡已用隔离数据库和归档目录核对。 [可信停止及独立读回](../state/acceptance/same-title-0.3.29-9c8bcf93-8362-4945-b867-55303aead62b.json) · [副本回退](../state/acceptance/fedora-signed-copy-rollback-0.3.29-field-finish-local-stop-only.jsonl) · [此前专用卡归档核对](../state/acceptance/fedora-0.3.29-native-stop-archive-verified.json) |
| Fedora 发现与部分读取 | 已安装副本无需登记，按每页一条读完两条本机会话，独立原生枚举验回同一组 ID。限制一页时界面报告读取未完成，将回退缓存设为只读，保留备注和版本，恢复配置后扫描成功。另在断网隔离的真实 Codex App Server 索引合成历史，覆盖 cli、vscode、appServer、两个提供方及非交互子代理排除。本次补验远端离线，跨端完整分页仍待本版复验。 [本机分页](../state/acceptance/fedora-0.3.29-discovery-local.log) · [部分读取和恢复](../state/acceptance/fedora-0.3.29-health-local.log) · [来源枚举](../state/acceptance/fedora-discovery-sources-0.3.29-offline.json) |
| Fedora 预览与跳转 | 从两张同名本机卡读回最近完整回复，并进入两个不同的原生会话 ID。保存版本和轮次保持，未发生创建或发送。真实进行中轮次显示上一条完整回复，只有阶段消息时提示暂无完整回复。同一归档的 MCP 界面夹具覆盖切卡或关闭后的迟到响应以及读取失败。专用卡已归档，隔离配置已恢复。 [原生回复和同名跳转](../state/acceptance/same-title-0.3.29-eb924b1a-8e31-4b2c-be5a-259d4507d6d6.json) · [浏览器夹具](../state/acceptance/fedora-mcp-gestures-nav-fixture-0.3.29-1795420097179.json) · [副本回退](../state/acceptance/fedora-signed-copy-rollback-0.3.29-field-finish-local-navigation.jsonl) |
| Fedora 账号切换界面 | 用随包脚本、隔离 Chrome 和两个模拟账号复验同 ID 卡片。同账号轮询保留草稿，换号清理旧编辑器、回复及归档，晚到的旧回执保持隐藏。返回旧账号只恢复属于该账号的草稿。空账号、切卡关闭及预览状态总计十项通过。夹具没有保存状态，也没有切换真实账号。 [浏览器回执](../state/acceptance/browser-account-switch-0.3.29.json) |
| Fedora 归档与批次恢复 | 筛选后批量归档本机专用卡，独立原生读回，关页重开仍有结果。关页时定时归档也由原生列表独立确认，停用后清除下次截止时间。另用隔离账本模拟两个待核对项，重启后保留模拟离线项，独立验回已提交的本机项。复验后取消归档，还原原生分区、标题、备注和配置。 [筛选后归档](../state/acceptance/fedora-archive-batch-0.3.29-1795423001418.jsonl) · [关页定时归档](../state/acceptance/fedora-archive-batch-0.3.29-1795423005650.jsonl) · [重启后核对](../state/acceptance/fedora-archive-reopened-checked-0.3.29.json) |
| Fedora 迁移与回退 | 在核对 86 个随包运行文件后，用冻结上游 schema 构建主库、远端分库和离线分库。三库的已提交 WAL 和整行数据在迁移快照中一致。运行时核对同 ID 跨主机、账号绑定、排序、待同步标题、归档回执及离线卡再发现。拒绝覆盖，输入文件字节一致，CLI 回退后服务停止投影旧数据。远端为合成夹具。 [迁移回执](../state/acceptance/fedora-migration-0.3.29.json) |
| Fedora 澄清与离线提示 | 隔离提供方经真实 App Server 完成包含澄清问题的答复。看板自动进入待审阅，从原生分区和回复读回后保持该状态、原人工备注。既有远端离线时，隔离 Chrome 中的实际看板显示异常及需处理的主机数。 [真机复验索引](../state/acceptance/fedora-0.3.29-local-clarification-retry.jsonl) · [离线提示回执](../state/acceptance/fedora-0.3.29-offline-health-web-retry.log) |
| Mac 原生 MCP App | 复验随包脚本、本机跨列和独立原生读回、刷新、边缘横滚及反拖复位。阻塞表单完成取消、空备注、三个旧版本拒绝和空草稿保留。可信标题拖动保持原生路由及预览状态，单击标题能读回完整回复，调整按钮只打开编辑器。 [拖放和表单](../state/acceptance/packages/macos-0.3.29-native-mcp-interactions-retry.jsonl) · [标题与控件](../state/acceptance/packages/macos-0.3.29-native-nav-retry.log) |
| Mac 搜索、键盘与重绘 | 标题及目录搜索保留所有状态列，空阻塞列可接收可信标题拖放。拖动和编辑跨过定时刷新时未重绘。键盘输入、Esc 取消和 Enter 保存通过，独立读回原分区和备注。保存期间控件锁定，手动和定时刷新保留列内位置。 [MCP 交互回执](../state/acceptance/packages/macos-0.3.29-native-mcp-filters-verified.jsonl) |
| Mac 安装与公共入口 | 官方 `26.1111.21308` 的独立副本通过安装、启动、重启和卸载。卸载验回官方签名、ASAR 和 plist。已登录副本的 HTTP、CLI 和 MCP 读回同一状态及回复，MCP 登记 18 项共享操作。系统 PWA 从隔离快捷方式两次独立启动并卸载。 [副本签名和原生读取](../state/acceptance/packages/macos-0.3.29-window-final.json) · [公共入口和 PWA](../state/acceptance/packages/macos-0.3.29-native-nav-retry.log) |
| Mac 远程与原地 | Mac→Fedora 到达已登记 SSH 目标，服务端接受所选公钥进入签名阶段，1Password SSH Agent 拒绝签名。Mac 远程移卡仍待验。macOS 原地模式仍缺官方身份和系统权限连续性。两平台最左侧固定图标直达仍待专用账号验证。 [当次 SSH 回执](../state/acceptance/packages/macos-0.3.29-fedora-ssh-diagnosis.txt) · [原包签名要求核验](../state/acceptance/packages/macos-0.3.28-signature-continuity.json) |
| Mac 补验通道 | 后续 Fedora→Mac 真机复验同样到达 SSH 签名阶段。限时验证在签名等待中结束，同一轮次探针的传输进程仍存活，尚未进入 Mac 测试副本。 [签名等待回执](../state/acceptance/fedora-0.3.29-mac-ssh-workflow-wait.json) |

- [Fedora x86_64 安装候选](../state/acceptance/packages/session-board-0.3.29-linux-x86_64.tar.gz "sha256=6e7794cccc761343fde19aac9694da33a7e999e651e5bfc98b27d8cf11887430") · [校验文件](../state/acceptance/packages/session-board-0.3.29-linux-x86_64.tar.gz.sha256)
- [macOS arm64 安装候选](../state/acceptance/packages/session-board-0.3.29-macos-arm64.tar.gz "sha256=fa145d5bbe59ad1eb7ee40b4d45396e8caa6deb90c9fbd1998287e7f7034e653") · [校验文件](../state/acceptance/packages/session-board-0.3.29-macos-arm64.tar.gz.sha256)

## 0.3.28 候选

2026 年 11 月 23 日从提交 `749f513` 构建。31 条冻结需求中 19 条达到双平台门槛，12 条待验，76 项自动门槛检查通过。原生拖放 `UI-001` 在 Fedora 和 macOS 均通过。 [逐项门槛](../state/acceptance/release-gate-0.3.28.json) · [现场证据索引](../state/acceptance/live-0.3.28.json)

| 场景 | 现场观察 |
| --- | --- |
| MacBook Pro 副本安装 | 安装器验证 OpenAI 签名、整份 ASAR、plist 中的归档头 SHA-256 和框架中唯一启用的字典摘要。仅在暂存副本修改启动入口，逐个重签 8 个 Helper 和 3 个 dylib，再签框架与主包。官方 `26.1111.21308` 的正常启动、重启与本机原生分区读取通过。卸载后副本恢复官方签名及原始 ASAR，系统应用的归档与 plist 前后一致。 [自动安装、重启及卸载](../state/acceptance/packages/macos-0.3.28-window-final.json) |
| Mac 安装中断与同步失败 | 在发布前、发布后和卸载交换中间强制结束安装器，分别验证可重装、已发布回执可补齐、旧补丁可恢复。回执重命名遇到持续 `EACCES` 时，现有补丁副本及其 inode 保持，解除后卸载成功。首次进程退出和实际回环端口占用均恢复官方包并保留诊断。 [真机故障注入](../state/acceptance/packages/macos-0.3.28-recovery-final.json) |
| Mac 更新、拖放与回退 | 从安装版 CLI 选择 `0.3.28`，重启后在已登录专用配置读回版本。把本机专用会话从进行中拖到待审阅，原生侧栏和独立 `thread/read` 读回一致。旧表单返回 HTTP 400，非空备注保持。反向拖放恢复原生分区，回退 `0.3.27` 后重启读回原分区和备注。测试后恢复原专用副本并关闭调试。 [更新命令回执](../state/acceptance/packages/macos-0.3.28-runtime-update.json) · [重启、拖放及回退](../state/acceptance/packages/macos-0.3.28-runtime-update-confirmed.json) |
| Mac 公共入口与 PWA | HTTP、随包 CLI 和 MCP stdio 读回同一本机会话、状态、备注及版本。MCP 登记 18 个共享操作，HTTP 与 MCP 预览读回同一轮，卡片数据保持一致。使用独立 Chrome 配置和本地调试管道完成系统安装。Chrome 在隔离用户目录生成 `Codex 会话看板.app`，通过系统快捷方式先后打开两次，两次均在独立窗口加载受 Service Worker 控制的看板。卸载清除了快捷方式。原专用副本已恢复，本轮候选已卸载。 [公共入口和系统 PWA 回执](../state/acceptance/packages/macos-0.3.28-system-pwa-standalone.jsonl) |
| Mac 原生 MCP App | 在隔离插件市场安装随包插件，从 Codex 插件详情打开原生 App。测试副本沿用现有 Surge 代理后成功渲染，内联脚本的 SHA-256 与归档一致。从标题将本机专用卡拖到待审阅，独立 `thread/read` 与备用网页读回目标。刷新保留非空备注，反向拖放恢复原分区。同列和拖出均未保存，右缘自动横滚 296 像素后可命中最右列。专用副本随后恢复，本轮候选已卸载。最左侧图标直达仍待验。 [原生拖放、读回和恢复](../state/acceptance/packages/macos-0.3.28-native-mcp-gestures-reconnect.jsonl) |
| Mac 原生阻塞表单 | 从专用卡的调整操作打开原生 MCP 表单。提示可留空，取消保留状态和备注。空白归一为空，省略备注保留原值，显式清除写入空字符串。拒绝旧保存、旧输入及旧位置版本，空草稿可重新核对后保存。独立验回专用卡原分区，候选已卸载。`UI-002` 四条现场断言已双平台通过，第一条仍缺自动断言。 [Mac 原生表单回执](../state/acceptance/packages/macos-0.3.28-native-mcp-forms-long-readiness.jsonl) |
| Mac 远程连接 | 已登录窗口读回原生本机卡片和三个已登记的 SSH 连接。Fedora 连接已启用，`thread/read` 和 `threadSection/list` 均超时。11 月 23 日复查同一目标，1Password SSH Agent 仍拒绝签名。当前候选的 Mac→Fedora 远程拖放待验。 [原生连接回执](../state/acceptance/packages/macos-0.3.27-signed-in-remote.json) · [当次 SSH 诊断](../state/acceptance/packages/macos-0.3.28-fedora-ssh-status.txt) |
| Mac 原地安装与发布签名 | `install --in-place` 回报仅支持 Fedora RPM，退出码为 1。系统原包的官方签名和启动归档仍一致。本地 ad hoc 副本通过严格验签；Gatekeeper 拒绝其分发签名，同机评估接受 OpenAI 公证原包。用原包自身的签名要求再次验签，原包通过，副本以退出码 3 拒绝。原地模式仍缺应用身份连续性、系统权限和更新恢复的实测证据。 [真机预检](../state/acceptance/packages/macos-0.3.28-in-place-preflight.json) · [分发签名评估](../state/acceptance/packages/macos-0.3.28-signing-assessment.json) · [原包签名要求核验](../state/acceptance/packages/macos-0.3.28-signature-continuity.json) |
| Fedora 现有副本 | 在已安装 `0.3.24` 的 Enhancer 侧栏点击更新，重启后从实际界面读回 `0.3.28`。本机与 MacBook Pro 两条原生连接均在线。回退后分别读取两张专用卡，分区、备注与标题保持，系统归档及隔离配置前后校验一致。 [界面更新及重启](../state/acceptance/fedora-signed-copy-update-0.3.28.jsonl) · [回退和双端独立读回](../state/acceptance/fedora-signed-copy-rollback-0.3.28.jsonl) |
| Fedora 原地与原生 MCP App | 在隔离 Fedora 44 中原地安装 `0.3.28`，从 RPM 原启动入口开启，从插件详情进入原生 App。本机及 MacBook Pro 两张专用卡跨列后均从实际主机读回，刷新保留非空备注。反向拖放恢复原分区。同列和拖出均未保存，右缘自动横滚到 396 像素的最右端。卸载复核原启动器、桌面项和 RPM 文件。最左侧图标直达仍待验。 [原地安装](../state/acceptance/in-place-rootless/signed-reinstall-0.3.28.txt) · [插件详情和随包校验](../state/acceptance/in-place-rootless/signed-native-mcp-click-0.3.28.jsonl) · [双端拖放](../state/acceptance/in-place-rootless/signed-native-mcp-drag-retry-0.3.28.jsonl) · [卸载](../state/acceptance/in-place-rootless/signed-final-uninstall-0.3.28.txt) |
| Fedora 分页、表单和系统 PWA | 从已登录的 `0.3.28` 副本复验双端分页，12 张活动卡与独立原生列表一致。分页截断时界面提示未完成，缓存保留备注及版本。隔离的真实 App Server 另验证交互来源和提供方覆盖。阻塞表单、旧版本拒绝、键盘和列内滚动复验通过。隔离 Chrome 生成系统桌面入口，先后两次打开独立窗口。两张专用卡与配置在回退后验回。 [分页](../state/acceptance/fedora-0.3.28-discovery.log) · [来源](../state/acceptance/fedora-discovery-sources-0.3.28.json) · [连接提示](../state/acceptance/fedora-0.3.28-health-retry.log) · [表单](../state/acceptance/fedora-ui-002-0.3.28-1795413184156.jsonl) · [键盘和滚动](../state/acceptance/fedora-ui-003-0.3.28-1795413162171.jsonl) · [系统 PWA](../state/acceptance/fedora-pwa-installed-0.3.28.json) · [回退](../state/acceptance/fedora-signed-copy-rollback-0.3.28-field-finish.jsonl) |
| Fedora 六状态、轮次与标题 | 在 `0.3.28` 独立读回六个原生目标分区。自动清理卡在关闭看板，回退后通过 `0.3.24` 原生侧栏移除两个空的临时分区并复核目录和远端连接。两端保存立即同步标题，真实开始、批准后恢复、停止、暂时重试和正常澄清完成均有回执。前台保存与后台标题重试并发时通过预算检查。另一次重启未能同时读回专用卡与持久待重试项，该断言待复验。 [六状态](../state/acceptance/fedora-native-six-columns-0.3.28-1795414404121.json) · [清理](../state/acceptance/fedora-native-six-columns-0.3.24-1795415644709.json) · [扩展场景索引](../state/acceptance/fedora-0.3.28-extended-field.jsonl) · [重启复验](../state/acceptance/fedora-title-retry-0.3.28-a30b2857c.jsonl) |
| 双平台随包复验 | Fedora 通过 101 项 Node 测试，跳过 1 项 macOS 专用测试。MacBook Pro 通过 98 项，跳过 4 项 Linux 专用测试。两端各通过 10 项公共入口测试、冻结模型复验和 18 项门槛测试。 [Fedora](../state/acceptance/fedora-archive-0.3.28-final.log) · [MacBook Pro](../state/acceptance/packages/macos-0.3.28-package-final.log) |

- [Fedora x86_64 安装候选](../state/acceptance/packages/session-board-0.3.28-linux-x86_64.tar.gz "sha256=a6b38e842059eb953bf7139f6172acbf6c3b178d39968c4ec80871eff876275b") · [校验文件](../state/acceptance/packages/session-board-0.3.28-linux-x86_64.tar.gz.sha256)
- [macOS arm64 安装候选](../state/acceptance/packages/session-board-0.3.28-macos-arm64.tar.gz "sha256=dfb91c2793debe78d0aec8ebdf061de4a45cb8eb7fc8a0ab2e06d7d055b60b7f") · [校验文件](../state/acceptance/packages/session-board-0.3.28-macos-arm64.tar.gz.sha256)

## 0.3.26 候选

由 `6933599` 构建。31 项冻结需求中 18 项达到双平台门槛，13 项待验，76 项自动门槛检查通过。Fedora 本版通过 39/40 条现场断言，唯一待验项是从账号最左侧固定图标直达原生 MCP App。macOS 通过 3/40 条随包交付断言，旧版副本启动试验另列。 [逐项门槛](../state/acceptance/release-gate-0.3.26.json) · [现场证据索引](../state/acceptance/live-0.3.26.json)

| 场景 | 现场观察 |
| --- | --- |
| macOS 官方签名恢复 | 用留存的官方 `26.803.81509` 构建，在私有目录从 `0.3.26` 压缩包安装客户端副本，切换运行时到 `0.3.25`，回退至 `0.3.26`，随后卸载。恢复后 Team ID、CDHash、指定要求及 7,121 个包内条目的内容、权限、所有权和文件标志相符，严格验签通过。参考副本已有的扩展属性保持，恢复副本另有系统写入的 `com.apple.provenance`。此实验没有启动旧版桌面。 [候选包与签名回执](../state/acceptance/packages/macos-0.3.26-copy-lifecycle.json) |
| macOS 安装保护 | 目标系统 Codex `26.1111.21308` 仍因框架内嵌摘要在复制前停止。原应用签名和归档 SHA-256 一致。Mac 真机合成包另验证了安装回执写入失败后复原、签名备份缺失时卸载停止，以及补回备份后成功卸载。 [目标构建预检](../state/acceptance/packages/macos-0.3.26-copy-lifecycle.json) · [安装器故障回归](../state/acceptance/packages/macos-0.3.26-package-checks.log) |
| macOS 旧版桌面 | 官方 `26.803.81509` 的私有副本经随包启动命令打开。Loader、Runtime、preload 和渲染器均启动，本机 `threadSection/list` 读回 1 个分区，`thread/list` 读回空列表，`account/read` 确认为未登录。退出后卸载并重新通过严格签名验证。旧版侧栏分组与各主机 App Server 分区之间的映射需要已登录的专用环境核对。 [旧版启动回执](../state/acceptance/packages/macos-0.3.26-legacy-window-launcher.json) |
| Fedora 副本 | 从已登录副本点击更新并重启，从界面读回 `0.3.26`。HTTP、CLI、MCP 读回同一双端状态和回复，可信标题拖动通过。回退到 `0.3.24` 后，双端专用会话由各自主机独立读回，原分区、标题和备注保持。 [更新](../state/acceptance/fedora-signed-copy-update-0.3.26.jsonl) · [公共入口](../state/acceptance/fedora-entrypoints-0.3.26.json) · [手势](../state/acceptance/fedora-ui-gestures-0.3.26-1795397578913.jsonl) · [回退](../state/acceptance/fedora-signed-copy-rollback-0.3.26.jsonl) |
| Fedora 原地与原生 MCP App | 在隔离 RPM 环境从原图标启动，在已安装插件详情打开原生 MCP App，核对随包 iframe 和自定义 XDG 路径。两张专用卡跨列移动，刷新和反向移回后均独立读回位置及备注。卸载后原启动器、桌面项及 RPM 校验通过。 [原地安装](../state/acceptance/in-place-rootless/signed-reinstall-0.3.26.txt) · [插件详情入口](../state/acceptance/in-place-rootless/signed-native-mcp-click-0.3.26.jsonl) · [双端拖放](../state/acceptance/in-place-rootless/signed-native-mcp-drag-retry-0.3.26.jsonl) · [卸载](../state/acceptance/in-place-rootless/signed-final-uninstall-0.3.26.txt) |
| Fedora 扩展现场 | 独立读回六列互移、同轮审批恢复、原生中断和标题重试。分页、远端空白新建、PWA 系统安装和旧库迁移另有回执。试验后恢复专用卡和隔离配置。 [六列](../state/acceptance/fedora-native-six-columns-0.3.26-1795399521723.json) · [批准](../state/acceptance/desktop-approval-0.3.26-acc31092.jsonl) · [中断](../state/acceptance/fedora-interrupt-0.3.26-3c985175.jsonl) · [重启重试](../state/acceptance/fedora-title-retry-0.3.26-a5d9ec0a5.jsonl) · [远端新建](../state/acceptance/fedora-remote-composer-0.3.26-1795398570634.json) · [PWA](../state/acceptance/fedora-pwa-installed-0.3.26.json) · [迁移](../state/acceptance/fedora-migration-0.3.26.json) |
| 双平台随包复验 | Fedora 通过 100 项，跳过 1 项 macOS 专用测试。MacBook Pro 通过 97 项，跳过 4 项 Linux 专用测试。两端各通过 10 项公共入口、10 项冻结模型和 18 项门槛测试。 [Fedora](../state/acceptance/fedora-archive-0.3.26.log) · [MacBook Pro](../state/acceptance/packages/macos-0.3.26-package-checks.log) |

目标 macOS 客户端仍受启动归档完整性检查限制。旧版 `26.803.81509` 已通过未登录副本启动，原生侧栏与远端分区能否同步、已登录看板以及原地模式仍待验。Fedora 原生 MCP 最左侧图标直达仍需隔离账号。

- [Fedora x86_64 候选](../state/acceptance/packages/session-board-0.3.26-linux-x86_64.tar.gz) · [SHA-256](../state/acceptance/packages/session-board-0.3.26-linux-x86_64.tar.gz.sha256)
- [macOS arm64 候选](../state/acceptance/packages/session-board-0.3.26-macos-arm64.tar.gz) · [SHA-256](../state/acceptance/packages/session-board-0.3.26-macos-arm64.tar.gz.sha256)

## 0.3.25 候选

由 `f3c54ec` 构建。31 项冻结需求中，18 项达到双平台门槛，13 项仍有待验。76 项门槛自动检查通过。Fedora 通过 39/40 条现场断言，剩余一条是账号最左侧固定图标直达原生 MCP App。macOS 通过 3/40 条随包交付断言，37 条桌面现场断言待验。`releaseReady` 为 `false`。[逐项门槛](../state/acceptance/release-gate-0.3.25.json) · [现场证据索引](../state/acceptance/live-0.3.25.json)

| 场景 | 现场观察 |
| --- | --- |
| Fedora 原地双端移卡 | 同一台 Fedora 44 使用隔离容器安装真实 RPM，Codex `26.1111.71704`。安装前后均从 RPM 自带的 `.desktop` 打开 `/opt/codex-desktop/ChatGPT`，启动参数字节一致。保留专用登录、项目和本机、MacBook Pro 两端连接。从原窗口的 Enhancer 入口把两个专用卡从待审阅拖到进行中。两个目标分区均在原生侧栏显示，并由各自原生连接独立读回。反向拖放恢复分区，两条测试备注保持。[原版窗口](../state/acceptance/in-place-rootless/signed-stock-native.json) · [原地安装](../state/acceptance/in-place-rootless/signed-reinstall.txt) · [双端移卡和恢复](../state/acceptance/in-place-rootless/signed-live-drag.jsonl) |
| Fedora 原生 MCP App | 从已安装插件详情点击打开原生 App，核对发行包 iframe。原生子进程收到自定义 `XDG_STATE_HOME` 和有效看板目录，原生 App 与回环网页读回两个专用卡及备注。双端跨列、刷新与反向移回通过。同列和拖出均保留原版本，右缘自动横滚达到最右端。账号最左侧固定图标直达仍待专用账号验证。[打开插件](../state/acceptance/in-place-rootless/signed-native-mcp-click.jsonl) · [共享路径](../state/acceptance/in-place-rootless/signed-native-mcp-verified.jsonl) · [真实拖放](../state/acceptance/in-place-rootless/signed-native-mcp-drag-retry.jsonl) |
| Fedora 原地卸载 | 在同一隔离环境卸载后 `rpm -V` 通过，原启动器和桌面入口的 SHA-256 一致。从原图标启动并读回与原版基线相同的项目、登录类型和两端专用会话分区。升级提示仍以有桌面用户服务的 `0.3.24` 客体回执为准。[卸载回执](../state/acceptance/in-place-rootless/signed-final-uninstall.txt) · [原版再次启动](../state/acceptance/in-place-rootless/signed-final-stock-native.json) |
| Fedora 副本更新回退 | 已登录副本从看板实际点击更新，重启后从界面和 HTTP 读回 `0.3.25`。从安装版执行回退并重启，读回 `0.3.24`，两端原生连接、专用分区、标题和备注保持。系统归档和隔离配置的 SHA-256 一致。[更新](../state/acceptance/fedora-signed-copy-update-0.3.25.jsonl) · [回退与复验](../state/acceptance/fedora-signed-copy-rollback-0.3.25.jsonl) |
| Fedora 旧六状态 | 用原生侧栏新建缺少的待判断和已完成两个本机分区，专用卡依次进入六个分区并回到待审阅。独立读回每次原生位置，两个临时分区、看板配置及卡片恢复。操作期间从隔离副本的 SSH 开关临时断开远端，完成后复连。[跨列与清理](../state/acceptance/fedora-native-six-columns-0.3.25-1795395377472.json) · [断开及复连](../state/acceptance/fedora-native-six-connectivity-0.3.25-1795395376377.jsonl) |
| Fedora 原生轮次 | 已登录副本使用回环测试提供方触发原生批准请求。审批进入已阻塞，可信点击「允许一次」后同一轮实际执行固定测试命令并回到进行中。流错误报告 `willRetry` 时，原生分区和看板均保持进行中。普通澄清结束后进入待审阅，可信停止进入已阻塞。备注保持，临时会话归档。[批准与重试](../state/acceptance/desktop-approval-0.3.25-c045ca84.jsonl) · [澄清](../state/acceptance/fedora-title-001-live-0.3.25-1795393813569.json) · [停止](../state/acceptance/fedora-interrupt-0.3.25-fe06cf0a.jsonl) · [终态失败离线复验](../state/acceptance/fedora-offline-native-turns-0.3.25.json) |
| Fedora 标题同步 | 双端均在同次保存返回前读回新标题。测试数据库拒绝专用卡改名后，状态和备注先落盘，重启后仍有待同步项。解除限制后自动改名并清空队列。单轮原生预算为 3,000 毫秒，观测到超时结算用时 2,999.90 毫秒。后台等待旧回执时，前台保存耗时 157.92 毫秒，旧回执没有覆盖新待办。[双端读回](../state/acceptance/fedora-title-001-live-0.3.25-1795393518136.json) · [重启重试](../state/acceptance/fedora-title-retry-0.3.25-f216b8d74.jsonl) · [预算](../state/acceptance/fedora-title-003-ack-0.3.25-6c182f853.jsonl) |
| Fedora 发现、打开与新建 | 本机与远端按每页一条完整读回，12 张卡与两端原生 ID 集合一致。原生 App Server 索引三种合成交互来源和两个提供方标签，排除子代理。两个同名专用会话按各自主机和 ID 打开。远端专用项目打开空白原生输入页，重放同一操作后独立读回的会话清单一致。[分页](../state/acceptance/fedora-0.3.25-discovery.log) · [发现来源](../state/acceptance/fedora-discovery-sources-0.3.25.json) · [同名定位](../state/acceptance/fedora-nav-same-title-0.3.25-1795393511475.json) · [远端输入页](../state/acceptance/fedora-remote-composer-0.3.25-1795395512945.json) |
| Fedora 表单与预览 | 阻塞表单支持取消、显式清空和保留原备注，三个旧版本分别被拒。搜索保留空列，键盘可编辑保存。标题的同列拖动、拖出和边缘横滚均保持原生路由与预览状态。执行中读回上一条完整答复，随包 iframe 通过关闭、切卡后忽略迟到响应的控制试验。[表单](../state/acceptance/fedora-ui-002-0.3.25-1795393493341.jsonl) · [键盘](../state/acceptance/fedora-ui-003-0.3.25-1795393472800.jsonl) · [原生手势](../state/acceptance/fedora-ui-gestures-0.3.25-1795394582535.jsonl) · [执行中预览](../state/acceptance/fedora-title-001-live-0.3.25-1795393763566.json) · [回复控制试验](../state/acceptance/fedora-mcp-gestures-nav-fixture-0.3.25-1795393719095.json) |
| Fedora 归档与主机恢复 | 筛选后批量归档、关页重开以及关页定时归档均从原生连接读回，专用卡随后恢复。中断隔离副本自己的 SSH 传输时，远端卡暂为只读。相同窗口自动重连并读回原位置，日用连接在试验期间保持在线。[批量归档](../state/acceptance/fedora-archive-batch-0.3.25-1795393831984.jsonl) · [定时归档](../state/acceptance/fedora-archive-batch-0.3.25-1795393843425.jsonl) · [连接恢复](../state/acceptance/fedora-reconnect-0.3.25-1795393857592.jsonl) |
| Fedora 入口与迁移 | 隔离浏览器安装 PWA，从生成的系统启动项首次打开及关闭重开均控制同一版服务。HTTP、CLI、MCP 读回相同的双端状态、版本和同轮回复，随包登记 18 个共享操作。用冻结 schema 的合成 SQLite 主库、远端分库及已提交 WAL 验证迁移和回退。[PWA](../state/acceptance/fedora-pwa-installed-0.3.25.json) · [公共入口](../state/acceptance/fedora-entrypoints-0.3.25.json) · [迁移](../state/acceptance/fedora-migration-0.3.25.json) |
| 双平台解包 | Fedora 通过 100 项随包测试，MacBook Pro 通过 96 项、跳过 4 项 Linux 专用测试。两平台各通过 10 项公共入口、10 项冻结模型和 18 项门槛测试。Mac Codex `26.1111.21308` 的副本预检因框架内嵌 ASAR 摘要停止，原地预检因平台限制停止。系统应用签名与启动归档均通过复验。[Fedora](../state/acceptance/fedora-archive-0.3.25.log) · [MacBook Pro](../state/acceptance/packages/macos-0.3.25-preflight.txt) |

Fedora 剩余的原生 MCP 图标直达需要可写入侧栏偏好的专用账号。已登录隔离副本沿用现有账号，侧栏固定状态由账号共享，验收使用原生插件详情入口。

2026 年 11 月 23 日在 MacBook Pro 复验了系统应用的官方签名，以及签名资源清单记录的完整 ASAR 摘要和 `Info.plist` 记录的 ASAR 头摘要。13 个受查构建的启动入口都存放在启动归档内。副本仍需可通过完整性预检且具有所需原生接口的官方构建。macOS 原地模式还需实现官方签名和应用元数据的恢复，并在真机完成生命周期验收。[签名与入口回执](../state/acceptance/packages/macos-20261123-boot-entry-and-seals.md) · [历史官方构建预检](../state/acceptance/packages/macos-official-builds.md)

- [Fedora x86_64 归档](../state/acceptance/packages/session-board-0.3.25-linux-x86_64.tar.gz) · [SHA-256](../state/acceptance/packages/session-board-0.3.25-linux-x86_64.tar.gz.sha256)
- [macOS arm64 归档](../state/acceptance/packages/session-board-0.3.25-macos-arm64.tar.gz) · [SHA-256](../state/acceptance/packages/session-board-0.3.25-macos-arm64.tar.gz.sha256)

## 0.3.24 候选

由 `694c335` 构建。默认副本安装仍可用，Linux 包另含显式的 Fedora 原地模式。两个平台共 76 项门槛自动检查通过。31 项冻结需求中 18 项通过、13 项仍需本候选现场证据。Fedora 记录 4 条逐项现场断言，macOS 记录 3 条随包交付断言。原地安装回执单列。`releaseReady` 为 `false`。[逐项门槛](../state/acceptance/release-gate-0.3.24.json) · [现场证据索引](../state/acceptance/live-0.3.24.json)

| 场景 | 现场观察 |
| --- | --- |
| 安装、退出与原图标 | Fedora 44 KDE 隔离客体用真实 RPM 安装 `26.1111.71704`，SELinux enforcing。从 RPM 的原 `.desktop` 启动原 `/opt/codex-desktop/ChatGPT`，启动参数及窗口 class 与安装前一致，原生本机 App Server 连接初始化。看板加载 `0.3.24`，回环网页监听 `18768`。重复安装返回 `unchanged`。客体断网且未登录 Codex，已登录原窗口侧栏、本机和远端移卡仍需实测。[启动前对照](../state/acceptance/in-place-rpm/results/original-launch-verified-baseline.txt) · [原图标及连接](../state/acceptance/in-place-rpm/results/original-launch-final-024-patched.txt) · [运行时](../state/acceptance/in-place-rpm/results/enhancer-024-lifecycle.txt) |
| 安装失败、中断与竞争 | 原客户端运行时拒绝安装和卸载，安装前后 `/run/user/1000/codex-desktop/` 的两个监听 socket 路径一致。归档复制遇到操作系统文件大小限制后，原件指纹和元数据一致，系统安装目录已清理。首次记录落盘后杀掉安装器，重试成功。首次记录残留和 `umask 077` 下的安装也可重试。RPM 事务锁被占用时拒绝安装，回执缺失时拒绝卸载并保留备份。[运行中保护](../state/acceptance/in-place-rpm/results/enhancer-024-guard.txt) · [中断恢复](../state/acceptance/in-place-rpm/results/enhancer-024-recover.txt) · [竞争与回执缺失](../state/acceptance/in-place-rpm/results/enhancer-024-rename.txt) |
| 更新、回退与双模式隔离 | 旧副本包被原地协议拒绝，活动指针字节保持一致。从原图标读回更新演练版本 `0.3.24-rehearsal.1`，回退后读回 `0.3.24`，客户端补丁文件保持一致。原地模式存在时，可用默认命令新建并卸载独立副本，副本归档恢复为原件，系统原地归档和启动元数据保持一致。[更新和回退](../state/acceptance/in-place-rpm/results/enhancer-024-update.txt) · [副本共存](../state/acceptance/in-place-rpm/results/enhancer-024-lifecycle.txt) |
| 已登录副本及公共入口 | 已安装副本更新并重启，复用原有本机和 MacBook Pro 连接。普通侧栏启动能看到双端专用卡。HTTP、成品 CLI 与 MCP stdio 读回相同的双端卡片及版本，预览同轮最终回复，保存数据保持一致。本候选从原生插件详情点击打开后未观察到 MCP App webview，重试时窗口未在限时内恢复连接。隔离插件和配置已复原，原生 MCP App 拖放仍沿用 `0.3.23` 的回执。[原生连接](../state/acceptance/fedora-signed-native-0.3.24.txt) · [侧栏](../state/acceptance/fedora-enhancer-rail-0.3.24.json) · [公共入口](../state/acceptance/fedora-entrypoints-0.3.24.json) · [当前原生 App 尝试](../state/acceptance/fedora-native-mcp-run-0.3.24.txt) |
| 标题写入预算 | 在已登录副本暂留专用卡的第一次原生改名回执，原生会话和轮次读取保持工作。后台还在等待时，前台同卡保存用时 175.34 毫秒，备注和新待同步项已落盘。独立预算试验中，三秒共享预算扣除原生读取后给改名 2919 毫秒，标题同步约 2999.30 毫秒结束。迟到的旧回执未清除新任务，随后恢复专用卡及写入限制。[计时与复原](../state/acceptance/fedora-title-003-ack-0.3.24-447d9a4be.jsonl) |
| Fedora PWA 系统安装 | 隔离 Chrome 中通过看板与原生确认框安装。Chrome 生成的 `.desktop` 通过系统校验。通过该启动项首次打开和关闭后重开，两次均获得独立窗口和 Service Worker，读回同一 `0.3.24` 实例。看板配置、记录、归档账本及标题启用记录校验一致。[安装和重开](../state/acceptance/fedora-pwa-installed-0.3.24.json) |
| Loader 保留且构建变化 | 修改客体构建标签模拟混合替换。检查器报告 `incompatible`，原进程从原图标启动并初始化本机连接，Loader 给出版本和停用原因，看板运行日志无新条目。复原构建标签后状态恢复。真实登录、远程离线和短暂失败在原地模式待补。[停用及恢复](../state/acceptance/in-place-rpm/results/enhancer-024-lifecycle.txt) · [原进程日志](../state/acceptance/in-place-rpm/results/original-gtk-launch-final-024-incompatible.log) |
| RPM 覆盖入口及通知 | 桌面在线时执行真实 RPM 事务切换至 `26.1111.21308`，独立用户服务识别覆盖并发送通知。图形用户退出且 D-Bus 消失后同版本重装，下一次登录发出通知。用本地重打包 RPM 验证升级改名为 `.vite/build/rpm-lab-renamed.js` 后仍能识别覆盖，卸载后原图标能启动改名构建。该改名来自隔离夹具。[异构建替换](../state/acceptance/in-place-rpm/results/enhancer-024-lifecycle.txt) · [重装及重新登录](../state/acceptance/in-place-rpm/results/enhancer-024-late-notification.txt) · [改名升级](../state/acceptance/in-place-rpm/results/enhancer-024-rename.txt) |
| 卸载及数据 | 普通卸载恢复原件，覆盖后卸载保留 RPM 当前版本。归档、启动器、桌面入口的 SHA-256 和元数据一致，`rpm -V` 通过，升级监听为非活动状态。私有测试记录默认保留，安装存在时拒绝清理，卸载后可由普通用户清理。实际登录、项目、远程连接和已启用自动化的原地生命周期留待专用账号复验。[普通恢复](../state/acceptance/in-place-rpm/results/enhancer-024-recover.txt) · [替换及私有数据](../state/acceptance/in-place-rpm/results/enhancer-024-lifecycle.txt) |
| 双平台归档 | Fedora 解包通过 100 项随包测试。MacBook Pro 解包通过 96 项，跳过 4 项 Linux 专用测试。两端随包公共入口、冻结模型和门槛测试通过。macOS `26.1111.21308` 副本预检仍因框架内嵌 ASAR 摘要停止，原地模式也在写入前拒绝。Mac 原应用签名校验通过，启动归档指纹一致。[Fedora 解包](../state/acceptance/fedora-archive-0.3.24.log) · [MacBook Pro 预检](../state/acceptance/packages/macos-0.3.24-preflight.txt) |

原地复验使用无网卡的 Fedora Live KVM 客体、独立数据盘和本地 RPM。脚本覆盖运行中保护、故障恢复、升级、回退、卸载和通知。Fedora 日用应用的启动归档校验与 RPM 记录一致。已登录的原生双端移卡使用前一版副本，见[原生检查点](session-board-native-checkpoint.md)。[客体复验说明](../state/acceptance/in-place-rpm/lab-notes.md) · [现场脚本](../state/acceptance/in-place-rpm/fixtures/verify-enhancer-024.sh)

- [Fedora x86_64 归档](../state/acceptance/packages/session-board-0.3.24-linux-x86_64.tar.gz) · [SHA-256](../state/acceptance/packages/session-board-0.3.24-linux-x86_64.tar.gz.sha256)
- [macOS arm64 归档](../state/acceptance/packages/session-board-0.3.24-macos-arm64.tar.gz) · [SHA-256](../state/acceptance/packages/session-board-0.3.24-macos-arm64.tar.gz.sha256)

## 0.3.23 候选

双平台归档由提交 `c247e10` 构建。31 项冻结需求中 18 项通过、13 项待验，76 项门槛自动检查通过。Fedora 有 35 条逐项现场断言，macOS 有 3 条打包断言。现场证据按候选版本、归档及运行文件指纹绑定，`0.3.22` 的现场回执保留在原候选。`releaseReady` 为 `false`。[逐项门槛报告](../state/acceptance/release-gate-0.3.23.json) · [现场证据索引](../state/acceptance/live-0.3.23.json)

| 项目 | 结果 |
| --- | --- |
| 首次输入与归档核对 | 先保存或先观察，同一首条输入均增加一次输入版本，携带旧版本的表单会被拒。归档核对按卡片所属主机读回，离线条目保留待核对，其余已提交项继续核对。测试覆盖两个主机的同会话 ID、未缓存的归档结果和未提交项。[随包回归](../state/acceptance/fedora-input-archive-packaged-0.3.23.log) |
| Fedora 原生归档 | 已登录隔离窗口先原生归档本机专用卡。在隔离账本中准备两条待核对记录，模拟首条远端不可用。重启后看板保留离线项的待核对状态，并从本机原生列表确认第二项。核对结果已落盘，专用卡和隔离配置随后恢复。实际筛选归档和关页定时归档也通过。[重启核对](../state/acceptance/fedora-archive-reopened-0.3.23.json) · [筛选归档](../state/acceptance/fedora-archive-batch-0.3.23-1795381011169.jsonl) · [关页定时归档](../state/acceptance/fedora-archive-batch-0.3.23-1795381017802.jsonl) |
| Fedora 发现 | 断网夹具由 Fedora 随包原生 App Server 建索引。随包看板读回三个交互来源及两个提供方标签，排除子代理历史。提供方覆盖基于播种的历史元数据。已登录副本另验证本机和远端分页集合、分页上限及界面诊断。[发现来源回执](../state/acceptance/fedora-discovery-sources-0.3.23.json) · [双端分页](../state/acceptance/fedora-product-paging-0.3.23.json) · [界面诊断](../state/acceptance/fedora-health-0.3.23.json) |
| Fedora 安装与入口 | 从隔离副本的已安装网页点击更新，重启读回 `0.3.23`，回退后启动读回 `0.3.22`，卸载恢复启动归档并保留测试数据。已登录副本另验证侧栏入口与双端卡片，HTTP、CLI 与 MCP 读回相同的专用数据。[更新及回退](../state/acceptance/packages/fedora-installed-web-update-0.3.23.txt) · [Enhancer 入口](../state/acceptance/fedora-enhancer-rail-0.3.23.json) · [公共入口](../state/acceptance/fedora-entrypoints-0.3.23.json) |
| Fedora 拖放与表单 | 从已安装插件详情打开原生 MCP App，核对随包资源。远端专用卡跨列后，原生分区、MCP App 重读及备用网页均读回目标状态和原备注，反向拖放恢复原分区。同列与拖出松手均无写入，边缘横滚抵达最后一列。Enhancer 阻塞表单留空、取消和三种过期版本校验通过。搜索保留空目标列，键盘编辑和保存通过，刷新保留列内滚动位置。[原生 MCP](../state/acceptance/fedora-mcp-gestures-live-0.3.23-1795383036820.json) · [安装与恢复](../state/acceptance/fedora-native-mcp-install-0.3.23-retry3.jsonl) · [阻塞表单](../state/acceptance/fedora-ui-002-0.3.23-1795382579786.jsonl) · [搜索、键盘和滚动](../state/acceptance/fedora-ui-003-0.3.23-1795382532388.jsonl) |
| Fedora 标题与状态 | 双端专用卡手动保存后，在同次响应返回前读回原生完整标题及当前状态前缀。隔离数据库拒绝改名后，重启读回待同步项，解除故障后自动重试并读回原标题正文。四、五及旧六状态的列、导航和表单顺序通过。两台原生宿主仍缺待判断与已完成分区。[双端标题](../state/acceptance/fedora-title-001-live-0.3.23-1795381802632.json) · [标题重试](../state/acceptance/fedora-title-retry-0.3.23-29fcb198f.jsonl) · [状态顺序](../state/acceptance/fedora-state-001-0.3.23-1795381869379.json) |
| Fedora 轮次与预览 | 专用轮次执行时进入进行中，正常澄清答复完成后进入待审阅，原生停止后进入已阻塞并保留备注。原生双端同名卡各自打开原会话，预览与各自完整回复一致。运行中预览读回上一条完整回复，随包页面用受控响应覆盖切卡、关闭、无回复与读失败。[开始和完成](../state/acceptance/fedora-title-001-live-0.3.23-1795382664892.json) · [原生停止](../state/acceptance/fedora-interrupt-0.3.23-8a1c0b16.jsonl) · [同名跳转](../state/acceptance/fedora-nav-same-title-0.3.23-1795382484336.json) · [运行中预览](../state/acceptance/fedora-title-001-live-0.3.23-1795382592535.json) · [隔离浏览器](../state/acceptance/fedora-mcp-gestures-nav-fixture-0.3.23-1795382516102.json) |
| Fedora 断线重连 | 仅中断隔离副本的 SSH 传输，远端专用卡转为只读缓存，本机保持在线。Codex 在同一窗口重连后，看板与独立原生读取均恢复原会话和分区。日用窗口的连接保持在线。[断线与读回](../state/acceptance/fedora-reconnect-0.3.23-1795382768364.jsonl) |
| 迁移 | 从成品 CLI 重放合成本机、远端 SQLite 和已提交 WAL，再从回环看板读回离线只读卡、恢复后的同一 ID 及归档核对时间。回退后重开服务，导入记录不再展示，源文件校验保持一致。[迁移回执](../state/acceptance/fedora-migration-0.3.23.json) |
| 双平台解包 | Fedora 通过 91 项随包测试。MacBook Pro 通过 89 项，跳过 2 项 Linux 专用测试。两端各通过 10 项公共入口、10 项冻结模型和 18 项门槛测试。macOS 的 `26.1111.21308` 仍在复制前因框架内嵌 ASAR 摘要拒绝安装。继续测试的官方归档因下载中断，未获得新增的完整兼容结论。[Fedora 解包](../state/acceptance/fedora-archive-0.3.23.log) · [MacBook Pro 预检](../state/acceptance/packages/macos-0.3.23-preflight.txt) · [官方归档复查](../state/acceptance/packages/macos-official-followup-20261123.txt) |

Fedora 仍缺旧六状态的全列互移、批准后实际恢复执行、临时请求错误自动重试期间的状态、标题写入器繁忙时的并发保存预算，以及账号最左侧图标直达原生 MCP App。PWA 后续通过系统安装和独立窗口复验。此前用 SQLite 写锁测试 `0.3.22` 时，标题同步超时并保留待重试项，锁也使并发保存所需的原生会话读取超时。[PWA 系统安装](../state/acceptance/fedora-pwa-installed-0.3.23.json) · [SQLite 写锁回执](../state/acceptance/fedora-title-budget-0.3.22-78ba4c09f.jsonl)

- [Fedora x86_64 归档](../state/acceptance/packages/session-board-0.3.23-linux-x86_64.tar.gz) · [SHA-256](../state/acceptance/packages/session-board-0.3.23-linux-x86_64.tar.gz.sha256)
- [macOS arm64 归档](../state/acceptance/packages/session-board-0.3.23-macos-arm64.tar.gz) · [SHA-256](../state/acceptance/packages/session-board-0.3.23-macos-arm64.tar.gz.sha256)

## 0.3.22 候选

双平台归档由提交 `e8b24a6` 构建。31 项冻结需求中 18 项通过、13 项待验。75 项门槛自动检查通过，Fedora 有 34 条逐项现场断言，macOS 有 3 条打包断言。`releaseReady` 为 `false`。[逐项门槛报告](../state/acceptance/release-gate-0.3.22.json) · [现场证据索引](../state/acceptance/live-0.3.22.json)

| 项目 | 结果 |
| --- | --- |
| Enhancer 侧栏 | 已登录 Fedora 副本在普通布局显示入口。临时隐藏其他侧栏入口后，原生图标栏中只出现一个 Enhancer 按钮。点击后分别从本机和远端筛选读回专用卡，恢复布局时看板保持打开。[真机入口](../state/acceptance/fedora-enhancer-rail-0.3.22.json) · [8 项浏览器回归](../state/acceptance/fedora-host-mount-browser-0.3.22.json) |
| 迁移及回退 | 用主库、主机分库和已提交 WAL 的合成夹具复验成品 CLI 和回环服务。目录缺失或首次远端扫描失败时，已绑定的迁移卡仍为只读。模拟原生连接恢复后用同一卡片 ID 接回，备注、版本及待核对状态保留。已核对批次的时间可读回。回退后重新启动服务，导入卡不再出现，源文件保持原校验值。[迁移回执](../state/acceptance/fedora-migration-0.3.22.json) · [复验脚本](../state/acceptance/fedora-migration-0.3.22.mjs) |
| 标题重试 | 隔离数据库只拒绝专用卡的指定改名。状态、备注和待同步项一并保存。重启后恢复待同步项，解除故障后自动重试，独立原生读取核对标题、位置及版本。专用卡和配置已恢复。[改名及重启回执](../state/acceptance/fedora-title-retry-0.3.22-3b212658c.jsonl) |
| Fedora 真实轮次 | 本机专用卡开始执行时进入进行中，普通澄清答复完毕后进入待审阅。另一专用轮次经可信点击停止后，原生确认同一轮中断，卡片进入已阻塞并保留自动原因和人工备注。执行时的预览接口读回前一条完整回复。两侧手动保存也在同次响应前读回原生完整标题。[开始与完成](../state/acceptance/fedora-title-001-live-0.3.22-1795378744546.json) · [中断](../state/acceptance/fedora-interrupt-0.3.22-d0337198.jsonl) · [双端标题](../state/acceptance/fedora-title-001-live-0.3.22-1795378696064.json) |
| Fedora 配置与交互 | 四、五和旧六状态模式的列、导航与表单顺序通过。两台原生宿主仍缺待判断和已完成分区。阻塞草稿留空、取消、空白归一、分别拒绝三个过期版本，以及搜索、空列拖放、键盘和滚动保留均通过。每页一条时两端读回集合与原生分页一致，单页上限触发界面诊断。[状态](../state/acceptance/fedora-state-001-0.3.22-1795378437888.json) · [阻塞表单](../state/acceptance/fedora-ui-002-0.3.22-1795378404994.jsonl) · [搜索与键盘](../state/acceptance/fedora-ui-003-0.3.22-1795378416981.jsonl) · [分页](../state/acceptance/fedora-product-paging-0.3.22.json) · [诊断](../state/acceptance/fedora-health-0.3.22.json) |
| 原生 MCP 与查看回复 | 从已安装插件详情进入原生 MCP App。远端专用卡跨列后，原生 App 和备用网页均读回目标位置及备注，反向拖放恢复原位置。同列、拖出和边缘横滚期间无额外保存。两张同名专用卡打开各自的原生会话，回复与两端原生读取一致。随包 MCP 页面在隔离浏览器内通过迟到、无完整回复和失败的模拟返回检查。[原生 MCP](../state/acceptance/fedora-mcp-gestures-live-0.3.22-1795378537287.json) · [同名与回复](../state/acceptance/fedora-nav-same-title-0.3.22-1795378688368.json) · [模拟返回](../state/acceptance/fedora-mcp-gestures-nav-fixture-0.3.22-1795378499746.json) |
| Fedora 归档与远端恢复 | 在筛选后的看板归档专用卡，关页重开后读回原生归档结果。关闭看板时后台定时归档也能读回，专用卡随后取消归档并恢复。故意断开仅属于隔离副本的 SSH 传输后，远端卡暂时只读，Codex 在同一窗口自动重连，独立原生读取核对原分区。日用窗口的独立传输保持连接。[筛选后归档](../state/acceptance/fedora-archive-batch-0.3.22-1795378940686.jsonl) · [后台定时归档](../state/acceptance/fedora-archive-batch-0.3.22-1795378950125.jsonl) · [断线重连](../state/acceptance/fedora-reconnect-0.3.22-1795378960547.jsonl) |
| Fedora 安装和公共入口 | 从已安装备用网页点击更新，重启读回 `0.3.22`，回退后读回 `0.3.21`，卸载恢复临时副本的启动归档。专用备注文件校验一致。已登录副本运行 `0.3.22` 时，HTTP、CLI 和 MCP 读回相同的专用卡。浏览器由 Service Worker 控制，安装检查无错误，系统安装与独立窗口待验。[更新、回退和卸载](../state/acceptance/fedora-installed-0.3.22.txt) · [已登录副本](../state/acceptance/fedora-signed-start-0.3.22.txt) · [公共入口](../state/acceptance/fedora-entrypoints-0.3.22.json) · [PWA 检查](../state/acceptance/fedora-pwa-installability-0.3.22.json) |
| 双平台解包 | Fedora 通过 87 项随包测试。MacBook Pro 通过 85 项，跳过 2 项 Linux 专用测试。两端各通过 10 项公共入口、10 项冻结模型和 18 项门槛测试。macOS 的 `26.1111.21308` 因框架内嵌 ASAR 摘要在复制前拒绝安装，桌面安装、运行、更新和回退待验。[Fedora 解包](../state/acceptance/fedora-archive-0.3.22.log) · [MacBook Pro 解包与预检](../state/acceptance/packages/macos-0.3.22-preflight.txt) · [官方构建兼容性复查](../state/acceptance/packages/macos-official-builds.md) |

Fedora 冻结需求仍待实测旧六状态的全列互移、批准后实际恢复执行、临时请求错误自动重试时保持执行状态、改名写入器繁忙时的预算、全部交互来源与提供方的原生发现，以及账号最左图标直达原生 MCP App。PWA 另缺系统安装和独立窗口回执。

- [Fedora x86_64 归档](../state/acceptance/packages/session-board-0.3.22-linux-x86_64.tar.gz) · [SHA-256](../state/acceptance/packages/session-board-0.3.22-linux-x86_64.tar.gz.sha256)
- [macOS arm64 归档](../state/acceptance/packages/session-board-0.3.22-macos-arm64.tar.gz) · [SHA-256](../state/acceptance/packages/session-board-0.3.22-macos-arm64.tar.gz.sha256)

## 0.3.21 候选

双平台归档由提交 `cc7ed74` 构建。31 项冻结需求中 18 项通过、13 项待验，73 项门槛自动检查通过。Fedora 现场 27 条断言通过，macOS 现场通过 3 条归档断言。`releaseReady` 为 `false`。[逐项门槛报告](../state/acceptance/release-gate-0.3.21.json) · [现场证据索引](../state/acceptance/live-0.3.21.json)

| 项目 | 结果 |
| --- | --- |
| 定时归档配置 | 缩短周期会按新周期重新计算截止时间，停用及切换到旧六状态模式会清除截止时间。各宿主独立调度，恢复后使用下一周期边界。两台空宿主的隔离夹具复现了 `0.3.20` 的旧截止时间问题，`0.3.21` 通过全部 7 项检查。[原版复现](../state/acceptance/fedora-timer-configuration-0.3.20.json) · [修复后复验](../state/acceptance/fedora-timer-configuration-0.3.21.json) |
| Fedora 关页定时归档 | 关闭 Enhancer 看板后，仅允许归档本机专用卡。独立原生列表先读回归档结果，随后从看板接口核对该轮归档记录和原备注。停用后清除下一次截止时间，验收后取消归档，并恢复专用卡、写入限制及配置原字节。[后台定时归档回执](../state/acceptance/fedora-archive-batch-0.3.21-1795376234815.jsonl) |
| Fedora 原生操作 | 本机与远端专用卡同次保存返回前均独立读回原生新标题。筛选后的本机专用卡经看板归档，关闭重开仍读回结果。原生 MCP App 对远端专用卡进行跨列、刷新和反向拖放，备用网页与原生分区读回一致。备注保留，专用卡的原位置和标题已恢复。[标题读回](../state/acceptance/fedora-title-001-live-0.3.21-1795375974388.json) · [归档回执](../state/acceptance/fedora-archive-batch-0.3.21-1795375983506.jsonl) · [原生 MCP App](../state/acceptance/fedora-mcp-gestures-live-0.3.21-1795376077076.json) |
| Fedora 真轮次与状态模式 | 专用卡原生执行时自动进入进行中。普通澄清答复结束后进入待审阅。另一轮点击原生停止后进入已阻塞，自动原因与人工备注同时保留。四、五及旧六状态的列顺序、导航和表单一致。原生目录仍缺待判断与已完成分区。[澄清与完成](../state/acceptance/fedora-title-001-live-0.3.21-1795376971788.json) · [停止](../state/acceptance/fedora-interrupt-0.3.21-c34e24ac.jsonl) · [状态](../state/acceptance/fedora-state-001-0.3.21-1795377091818.json) |
| Fedora 分页、导航和表单 | 每页一条时，本机及远端共读回 12 张卡，独立原生集合一致。限制至一页后，诊断报告未完成，已保存数据保留。同名专用卡均定位到正确主机和会话，原生轮次不变。搜索、空列拖放和键盘编辑通过。[分页](../state/acceptance/fedora-product-paging-0.3.21.json) · [诊断](../state/acceptance/fedora-health-0.3.21.json) · [导航](../state/acceptance/fedora-nav-same-title-0.3.21-1795377143491.json) · [键盘与筛选](../state/acceptance/fedora-ui-003-0.3.21-1795377121630.jsonl) |
| Fedora 改名失败 | 隔离数据库拒绝专用卡改名时，原生状态和备注保存，界面显示标题同步待重试。测试副本在临时授权配置下重启后，未在观察窗口内读回原生连接，重试轨迹待复验。临时限制已撤除，专用卡原位置和标题已恢复。[保存与等待](../state/acceptance/fedora-title-retry-0.3.21-8fce2ad58.jsonl) · [恢复](../state/acceptance/fedora-title-retry-cleanup-0.3.21-8fce2ad58.json) |
| Fedora 安装生命周期 | 从已安装备用网页点击更新，重启读回 `0.3.21`，回退并重启后读回 `0.3.20`。卸载后试验副本启动归档与系统归档一致，专用备注夹具校验通过。已登录隔离副本也独立加载 `0.3.21`，HTTP、CLI 和 MCP 读回同一份双端数据及原生回复。[网页更新及回退](../state/acceptance/fedora-installed-0.3.21.txt) · [公共入口回执](../state/acceptance/fedora-entrypoints-0.3.21.json) |
| 双平台解包 | Fedora 通过 85 项随包测试，MacBook Pro 通过 83 项并跳过 2 项 Linux 专用测试。两端各通过 10 项公共入口、10 项冻结模型和 18 项门槛测试，运行文件与随包清单保持一致。macOS Codex `26.1111.21308` 仍在复制前因框架内嵌 ASAR 摘要拒绝安装。[Fedora 解包结果](../state/acceptance/fedora-archive-0.3.21.log) · [MacBook Pro 测试与预检](../state/acceptance/packages/macos-0.3.21-preflight.txt) |

- [Fedora x86_64 归档](../state/acceptance/packages/session-board-0.3.21-linux-x86_64.tar.gz) · [SHA-256](../state/acceptance/packages/session-board-0.3.21-linux-x86_64.tar.gz.sha256)
- [macOS arm64 归档](../state/acceptance/packages/session-board-0.3.21-macos-arm64.tar.gz) · [SHA-256](../state/acceptance/packages/session-board-0.3.21-macos-arm64.tar.gz.sha256)

## 0.3.20 候选

双平台归档由提交 `3a808ce` 构建。31 项冻结需求中 18 项通过、13 项待验，73 项门槛自动检查通过。Fedora 现场 15 条断言通过，macOS 现场通过 3 条归档断言。`releaseReady` 为 `false`。[逐项门槛报告](../state/acceptance/release-gate-0.3.20.json) · [现场证据索引](../state/acceptance/live-0.3.20.json)

| 项目 | 结果 |
| --- | --- |
| 归档批次 | 重放登记要求目标顺序、会话 ID、标题和三个版本一致。已经登记的旧分区版本仍可重放。批次结束后仍在预检的卡片保持未提交，已经提交的卡片可以继续收取原生回执。[归档回归](../src/extensions/session-board/archive.test.ts) · [旧版批次回归](../src/extensions/session-board/discovery.test.ts) |
| Fedora 原生归档 | 从已登录 Enhancer 将筛选后的单张本机专用卡批量归档，独立原生列表读回已归档，备注保留，关页重开后结果仍在。先前结束的专用批次保留未提交状态，冲突登记和继续提交均被拒，独立原生列表仍读回活动卡。验收后从宿主取消归档，原分区、标题、备注和输入版本读回一致。[原生归档回执](../state/acceptance/fedora-archive-batch-0.3.20-1795375127265.jsonl) |
| Fedora 原生 MCP App | 从已安装插件详情打开 App，核对 iframe 内容摘要。远端专用卡跨列后，原生分区、App 刷新及备用网页均读回目标位置和原备注。反向拖放恢复原标题和分区。同列和拖出松手均无保存，缩窄视口后边缘横滚 296 像素。重启隔离副本时本机和远端连接分别恢复。账号最左侧图标待验。[手势回执](../state/acceptance/fedora-mcp-gestures-live-0.3.20-1795375210040.json) · [重启回执](../state/acceptance/fedora-native-relaunch-connections-0.3.20.json) · [隔离配置恢复](../state/acceptance/fedora-native-mcp-window-restored-0.3.20.json) |
| Fedora 同窗口断线重连 | 中断仅归隔离副本所有的本机 SSH 传输后，远端专用卡转为只读且不可打开。本机连接保持可用。Codex 在同一窗口自动建立新传输，远端扫描恢复，专用卡恢复可打开。独立 `thread/read` 及看板均读回原位置，备注和版本保持一致。日用窗口的独立传输保持连接。[断线重连回执](../state/acceptance/fedora-reconnect-0.3.20-1795375570702.jsonl) |
| Fedora 表单与拖放 | 阻塞表单允许留空和取消，三个陈旧版本分别被拒。标题和目录筛选保留空目标列，支持键盘编辑和保存。拖动或编辑期间暂停刷新，列内滚动保留。标题同列拖动、拖出看板及边缘横滚期间无额外保存、预览或原生跳转。[阻塞表单](../state/acceptance/fedora-ui-002-0.3.20-1795374838478.jsonl) · [筛选与键盘](../state/acceptance/fedora-ui-003-0.3.20-1795374851867.jsonl) · [标题拖动](../state/acceptance/fedora-ui-gestures-0.3.20-1795374873378.jsonl) |
| Fedora 公共入口与 PWA | HTTP、CLI 和 MCP 读回同一本机与远端专用卡的状态、备注及三个版本。HTTP 与 MCP 预览返回同轮回复，预览前后保存状态一致。隔离 Chrome 受 Service Worker 控制，可安装性检查无错误，系统安装和独立窗口待验。[公共入口回执](../state/acceptance/fedora-entrypoints-0.3.20.json) · [浏览器回执](../state/acceptance/fedora-pwa-installability-0.3.20.json) |
| Fedora 安装生命周期 | 从已安装备用网页点「更新」，两次重启分别读回 `0.3.20` 和回退后的 `0.3.19`。卸载恢复临时副本的启动归档，专用备注夹具及系统归档通过校验。已登录隔离副本也加载了 `0.3.20`。[网页更新及回退](../state/acceptance/fedora-installed-0.3.20.txt) · [已登录副本](../state/acceptance/fedora-signed-start-0.3.20.txt) |
| 双平台解包 | 直接运行 `npm test` 后，发行清单和运行文件指纹仍一致。Fedora 通过 84 项。MacBook Pro 通过 82 项，跳过 2 项 Linux 专用测试。两端各通过 10 项公共入口、10 项冻结模型和 18 项门槛测试。macOS Codex `26.1111.21308` 的 Framework 内嵌 ASAR 摘要，安装在复制前停止。[Fedora 随包复验](../state/acceptance/fedora-archive-0.3.20.log) · [MacBook Pro 测试与预检](../state/acceptance/packages/macos-0.3.20-preflight.txt) |

- [Fedora x86_64 归档](../state/acceptance/packages/session-board-0.3.20-linux-x86_64.tar.gz) · [SHA-256](../state/acceptance/packages/session-board-0.3.20-linux-x86_64.tar.gz.sha256)
- [macOS arm64 归档](../state/acceptance/packages/session-board-0.3.20-macos-arm64.tar.gz) · [SHA-256](../state/acceptance/packages/session-board-0.3.20-macos-arm64.tar.gz.sha256)

## 0.3.19 候选

双平台归档由提交 `8ee8b3b` 构建。31 项冻结需求中 18 项通过、13 项待验，73 项门槛自动检查通过。此版 Fedora 现场断言通过 19 条。`0.3.18` 的 30 条现场断言仍绑定原候选。`releaseReady` 为 `false`。[逐项门槛报告](../state/acceptance/release-gate-0.3.19.json) · [现场证据索引](../state/acceptance/live-0.3.19.json)

| 项目 | 结果 |
| --- | --- |
| 远端新建 | 0.3.18 对同一个仍在等待导航的新建操作发出了两次原生打开请求。0.3.19 合并同账号、同操作 ID 和同项目的同时请求。重复请求只打开一次，新建登记完成后可用原操作 ID 继续打开输入页。项目或主机不匹配会拒绝。已用安装版相同的 `BoardService` 请求入口复验。[新建回归](../src/extensions/session-board/project-assignment.test.ts) |
| 安装失败恢复 | 首次安装或升级 Loader 时，注入安装回执写入失败，客户端副本分别恢复原始归档或上一版归档、链接和回执。恢复后可以重试。Fedora 和 MacBook Pro 的合成夹具都通过。进程被强制终止或断电的恢复尚未验证。[安装回归](../src/installer/install.test.ts) · [MacBook Pro 回执](../state/acceptance/packages/macos-0.3.19-preflight.txt) |
| Fedora 原生标题重试 | 仅在隔离本机数据库中拒绝专用卡的目标标题。原生分区先保存，旧标题、备注和已保存版本保持有效，Enhancer 显示标题同步待重试。重启隔离副本仍有同一待办。撤除约束后，观察器自动写入并独立读回原生标题，待办清除。测试后恢复专用卡原分区和标题，两份配置与系统归档校验一致。[原生标题重试](../state/acceptance/fedora-title-retry-0.3.19-e9bb5acb2.jsonl) |
| Fedora 安装生命周期 | 另起回环网络隔离的 Fedora 副本，从已安装备用网页点击更新，重启依次读回 `0.3.18 → 0.3.19 → 0.3.18`。卸载恢复原始归档，持久夹具和系统归档校验一致。已登录隔离副本也加载了 `0.3.19`。[网页更新和回退](../state/acceptance/fedora-installed-0.3.19.txt) · [已登录副本](../state/acceptance/fedora-signed-start-0.3.19.txt) |
| Fedora 已有连接分页 | 在运行中的候选设每页 1 条，本机读回 2 张，远端读回 10 张。两端读到终止游标，ID 集合与独立原生分页一致，原配置和集合随后恢复。现场数据只覆盖 `vscode` 和 `openai`。[分页回执](../state/acceptance/fedora-product-paging-0.3.19.json) |
| Fedora 状态诊断和同名导航 | 限制总页数后，真实界面报告远端列表未完成、自动状态尚未连接，专用卡版本及备注保留。两端专用卡使用相同标题时，预览各自的回复并打开正确主机和会话。操作期间无新输入或轮次，原生会话集合及卡片版本一致，结束时恢复原标题。[诊断回执](../state/acceptance/fedora-health-0.3.19.json) · [同名导航](../state/acceptance/fedora-nav-same-title-0.3.19-1795373499457.json) |
| Fedora 立即改名 | 本机与远端专用卡同次保存返回前均读回新标题。最新原生标题的正文、长文本、表情和其他标签保留，只有开头的状态前缀更新。随后恢复原位置和原标题。[标题回执](../state/acceptance/fedora-title-001-live-0.3.19-1795373526786.json) |
| Fedora 原生 MCP App | 从已安装插件详情打开 App。远端专用卡跨列后，独立原生连接、App 刷新及备用网页读回目标分区和原备注，反向拖回也一致。同列和拖出松手无保存。缩窄视口后横滚 296 像素并到达最右列。重启隔离副本后，本机和远程连接均重新建立，两张专用卡仍保留原分区与备注。账号最左侧快捷入口待验。[拖放回执](../state/acceptance/fedora-mcp-gestures-live-0.3.19-1795374230143.json) · [重启读回](../state/acceptance/fedora-native-mcp-restart-0.3.19.json) · [隔离配置恢复](../state/acceptance/fedora-native-mcp-window-restored-0.3.19.json) |
| 双平台解包 | Fedora 通过 80 项随包 Node 测试，MacBook Pro 通过 78 项并跳过 2 项 Linux 专用测试。两端各通过 10 项公共入口、10 项冻结模型和 18 项门槛测试。Mac 的门槛夹具需使用规范路径形式的 `TMPDIR`。Mac Codex `26.1111.21308` 仍在复制前因 ASAR 完整性预检退出。[Fedora 随包复验](../state/acceptance/fedora-archive-0.3.19.log) · [MacBook Pro 复验](../state/acceptance/packages/macos-0.3.19-preflight.txt) |

在解包目录用 `INSTALL.md` 的命令直接运行随包测试。源码的 `npm test` 会重新构建并写入工作区版本，重新构建后的目录不再匹配原发行清单。Fedora 复验使用了另一份未重新构建的解包目录。Vue 类型检查在安装构建依赖的目录运行。[原始归档测试](../state/acceptance/fedora-archive-0.3.19.log) · [Vue 类型检查](../state/acceptance/fedora-types-0.3.19.log)

## 0.3.18 候选

双平台归档由提交 `93e6c2e` 构建。31 项冻结需求中 18 项通过、13 项待验，71 项门槛自动检查通过，Fedora 通过 30 条现场断言。`releaseReady` 为 `false`。[逐项门槛报告](../state/acceptance/release-gate-0.3.18.json) · [现场证据索引](../state/acceptance/live-0.3.18.json)

| 项目 | 结果 |
| --- | --- |
| 构建与审计 | Fedora 和 MacBook Pro 均从干净解包通过 `npm ci`、`npm run typecheck:ui`。后端编译用 TypeScript `7.0.2`，`vue-tsc` 用 `6.0.3` 检查 Vue 单文件组件。构建时运行 Vue 类型检查，依赖安装不会复制客户端。预览解析器更新为 `markdown-it` `14.3.3`，两端 `npm audit` 均报告零项漏洞。[Fedora 类型检查](../state/acceptance/fedora-clean-vue-0.3.18.log) · [Fedora 审计](../state/acceptance/npm-audit-0.3.18.json) · [MacBook Pro 回执](../state/acceptance/packages/macos-0.3.18-preflight.txt) |
| Fedora 安装与回退 | 从已安装网页点击更新，重启依次读回 `0.3.17 → 0.3.18 → 0.3.17`。卸载恢复测试副本启动归档，持久数据夹具通过校验。原登录隔离副本也升级至 `0.3.18`。[安装回执](../state/acceptance/fedora-installed-0.3.18.txt) · [已登录副本](../state/acceptance/fedora-signed-start-0.3.18.txt) |
| Fedora 最近回复与导航 | 本机和远端专用卡临时改为同一个原生标题后，仍各自读回对应的完整助手回复，并按主机和 ID 选中原生会话。操作前后的会话集合、最新轮次及卡片版本一致，随后恢复原标题。轮次真实执行期间，从 Enhancer 点击卡片后看到「上一条完整回复 · 当前轮次进行中」，正文与上一轮一致，关闭时保持原路由。迟到回复、无完整回复和读取失败提示通过独立浏览器夹具验证，已登录宿主现场待验。[同名跳转](../state/acceptance/fedora-nav-same-title-0.3.18-1795371315275.json) · [执行中预览](../state/acceptance/fedora-title-001-live-0.3.18-1795371731691.json) · [预览夹具](../state/acceptance/browser-preview-cases-0.3.18.json) |
| Fedora 公共入口与 PWA | HTTP、CLI、MCP 读回两端专用卡的同一状态、版本和备注。HTTP 和 MCP 的回复一致，预览前后卡片版本一致。MCP 列出 18 个共享操作及看板入口。隔离 Chrome 的 Service Worker 获得控制，浏览器报告无可安装性错误。系统安装、独立窗口和最左侧快捷入口待验。[公共入口](../state/acceptance/fedora-entrypoints-0.3.18.json) · [浏览器回执](../state/acceptance/fedora-pwa-installability-0.3.18.json) |
| Fedora 原生 MCP App | 从隔离 Codex 的已安装插件详情打开随包 App，实际 iframe 内容摘要匹配。远端专用卡跨列后，原生连接、App 刷新及备用网页均读回目标分区和原备注，反向拖回也一致。同列、拖出松手均未保存。缩窄视口后右缘自动横滚 296 像素并到达最右列。隔离市场与配置已恢复，账号最左侧快捷入口待验。[拖放回执](../state/acceptance/fedora-mcp-gestures-live-0.3.18-1795371540856.json) · [安装及恢复](../state/acceptance/fedora-native-mcp-install-0.3.18.jsonl) |
| Fedora 分页与诊断 | 候选每页设为 1 条时，完整读回本机 2 张和远端 10 张，卡片 ID 集合与独立原生分页一致，原配置已恢复。现场只出现 `vscode` 和 `openai`，多来源及重复游标由自动测试覆盖。将总页数限制为一页时，远端诊断显示暂时无法更新，9 张缓存卡只读，两张专用卡的状态、备注及版本一致。恢复配置后扫描完整。[候选分页](../state/acceptance/fedora-product-paging-0.3.18.json) · [诊断回执](../state/acceptance/fedora-health-0.3.18.json) |
| Fedora 三种状态模式 | 四、五、六状态模式的看板列、顶部跳转和编辑选项名称、标识及顺序一致。六状态模式中「待归档」在「已完成」之前。本机和远端缺少原生「待判断」与「已完成」分区，涉及这两列的真实移卡待验。[状态回执](../state/acceptance/fedora-state-001-0.3.18-1795370543907.json) |
| Fedora 原生标题与真实轮次 | 两端专用卡手动移入「进行中」后，同次保存返回前读回新标题。表情、其他标签、正文状态词及长正文保留。本机专用轮次的原生开始、完成通知带同一轮次 ID，观察器自动读回「进行中」和「待审阅」原生分区及对应标题，备注保留。结束时恢复专用卡原标题和原分区。改名请求未落地便失败的情况已由隔离持久化夹具验证重启重试。[手动保存](../state/acceptance/fedora-title-001-live-0.3.18-1795370502136.json) · [自动状态和标题](../state/acceptance/fedora-title-001-live-0.3.18-1795370859190.json) · [重试测试](../state/acceptance/title-retry-0.3.17.log) |
| Fedora 中断与澄清 | 点击原生「停止」后，独立读回同一轮次已中断，看板自动移入「已阻塞」并显示原因。另一轮普通答复包含说明和澄清问题，正常结束后自动回到「待审阅」。两次均核对原生分区并保留人工备注，验收后恢复专用卡原位置和原标题。[中断回执](../state/acceptance/fedora-interrupt-0.3.18-a9667f2d.jsonl) · [澄清回执](../state/acceptance/fedora-title-001-live-0.3.18-1795371360189.json) |
| Fedora 表单与拖放 | 已复验阻塞表单的留空、取消、清空及陈旧版本拒绝。标题同列拖动、拖出看板及边缘横滚均无额外保存或原生跳转。键盘状态选择、Esc 取消、Enter 保存，以及拖动和编辑时的刷新锁定通过。[阻塞表单](../state/acceptance/fedora-ui-002-0.3.18-1795370591255.jsonl) · [筛选与键盘](../state/acceptance/fedora-ui-003-0.3.18-1795370608417.jsonl) · [标题拖动](../state/acceptance/fedora-ui-gestures-0.3.18-1795370631503.jsonl) |
| 换号回归 | 从归档解出的网页在隔离 Chrome 通过五项回归，覆盖同账号刷新、同 ID 换号、迟到回执及空账号。[浏览器回执](../state/acceptance/browser-account-switch-0.3.18.json) |
| 双平台干净解包 | Fedora 通过 77 项随包 Node 测试、10 项公共入口测试及 18 项门槛测试。MacBook Pro 通过 75 项随包 Node 测试，跳过 2 项 Linux 专用项，通过 10 项公共入口测试。两端各通过 10 项冻结模型检查。Mac Codex `26.1111.21308` 仍在复制前因 ASAR 完整性预检停止，系统归档校验一致。[Fedora 记录](../state/acceptance/fedora-clean-0.3.18.log) · [MacBook Pro 记录](../state/acceptance/packages/macos-0.3.18-preflight.txt) |

直接在归档中运行 `npm run lint` 需要 Git 忽略文件。归档中的受测源码使用 Biome 的 `--vcs-enabled=false` 对显式源码路径复验。[归档源码检查](../state/acceptance/fedora-clean-vue-0.3.18.log)

## 0.3.16 候选

双平台归档由提交 `b0011f1` 构建。31 项冻结需求中 18 项通过、13 项待验，69 项门槛自动检查全部通过，`releaseReady` 为 `false`。[逐项门槛报告](../state/acceptance/release-gate-0.3.16.json) · [现场证据索引](../state/acceptance/live-0.3.16.json)

| 项目 | 结果 |
| --- | --- |
| 换号后的界面 | 从归档解出的页面在隔离 Chrome 中通过五项换号回归。两个模拟账号使用同一卡片 ID。当前账号内轮询保留编辑框和草稿，换号关闭编辑框、清除回复预览并重新读取归档。迟到的旧账号预览和归档回执保持隐藏，切到空账号也能清理。切回原账号只恢复带账号标记的草稿。旧草稿原数据保留。换号前后使用同一个 PWA 安装按钮节点。[新版回归](../state/acceptance/browser-account-switch-0.3.16.json) · [0.3.15 复现](../state/acceptance/browser-account-switch-baseline-0.3.15.json) · [PWA 按钮复验](../state/acceptance/fedora-account-browser-0.3.16.json) |
| Fedora 已登录副本 | 升级至 `0.3.16` 后，从原窗口现有连接读回本机和 MacBook Pro 专用卡。看板快照和两张卡的账号标记一致。旧式无账号分区版本的保存被拒，远程专用卡的状态、备注和版本保持一致。[宿主回执](../state/acceptance/fedora-scope-0.3.16.json) |
| Fedora 公共入口 | 同一已登录副本通过 HTTP、随包 CLI 和 MCP 读回两张专用卡的同一状态、备注和版本。HTTP 和 MCP 返回相同最近回复。预览前后持久状态一致。MCP 列出 18 个共享操作和一个应用入口。`check-sections` 读回四个本机原生分区。[入口复验](../state/acceptance/fedora-entrypoints-0.3.16.json) · [分区预检](../state/acceptance/fedora-check-sections-0.3.16.json) |
| Fedora 安装与回退 | 从已安装网页看板点击更新，重启依次读回 `0.3.15 → 0.3.16 → 0.3.15`。卸载恢复测试副本启动归档，持久数据夹具通过 SHA-256 校验。[安装回执](../state/acceptance/fedora-installed-0.3.16.txt) |
| PWA 可安装性 | 隔离 Chrome 加载已登录副本提供的网页，Service Worker 获得控制，浏览器未报告可安装性错误。系统安装及独立窗口启动仍待验。[浏览器回执](../state/acceptance/fedora-pwa-installability-0.3.16.json) |
| Fedora 原生 MCP App | 从隔离 Codex 的插件详情打开实际原生 App，随包和已加载资源签名相符。装载专用卡白名单后，可信拖动完成跨列、刷新读回和反向恢复。备用网页和本机原生连接分别核对，备注保留。普通同列或拖出松手均无预览、跳转或保存。缩窄看板后，右缘自动横滚 296 像素并到达最右列。临时白名单已恢复并重启读回。最左侧置顶快捷方式仍待验。[原生回执](../state/acceptance/fedora-mcp-gestures-live-0.3.16-1795368997421.json) · [配置恢复](../state/acceptance/fedora-native-mcp-preconfigured-0.3.16.jsonl) · [夹具回执](../state/acceptance/fedora-mcp-gestures-fixture-0.3.16-1795367754707.json) |
| Fedora 阻塞表单 | Enhancer 中可信拖放验证跨列和同列表单、取消、留空、纯空白、填写及清空。第二入口先保存后，旧表单被拒且空草稿保留。错误输入和位置版本也被拒。目标主机读回原分区，备注和临时白名单恢复。[表单回执](../state/acceptance/fedora-ui-002-0.3.16-1795369070438.jsonl) |
| Fedora 筛选与键盘 | 标题及工作目录搜索保留目标列，可信拖动可落到筛选后的空列。状态下拉框响应方向键，Esc 取消，Enter 保存。跨过一次轮询时，拖动和编辑状态的看板节点均未变动。保存时筛选锁定，确认后解锁。手动与周期刷新均保留滚动位置和当前卡片节点。[交互回执](../state/acceptance/fedora-ui-003-0.3.16-1795369102668.jsonl) |
| Fedora 连接诊断 | 临时将扫描限制为一页。远端未完整发现的 9 张卡显示为只读，详情报告分页上限和自动状态未连接。两端专用卡的状态、备注及版本保留。恢复隔离配置后，远端重新完成扫描。[连接诊断回执](../state/acceptance/fedora-health-0.3.16.json) |
| 双平台干净解包 | Fedora 通过 75 项随包 Node 测试、10 项公共入口测试及 18 项门槛测试。MacBook Pro 通过 73 项随包 Node 测试，跳过 2 项 Linux 专用测试，通过 10 项公共入口测试。两端各通过 10 项冻结模型检查。Mac Codex `26.1111.21308` 的完整性预检在复制前停止，系统归档的 SHA-256 前后一致。[Fedora 测试记录](../state/acceptance/fedora-clean-0.3.16.log) · [MacBook Pro 测试和预检](../state/acceptance/packages/macos-0.3.16-preflight.txt) |
| 独立 Vue 类型检查 | `vue-tsc` 启动时找不到 TypeScript 7 的 `typescript/lib/tsc` 导出。单独运行 `tsc` 无法解析三个 Vue 单文件组件。随包构建及浏览器回归通过。[类型检查输出](../state/acceptance/frontend-typecheck-0.3.16.log) |

换号回归使用隔离 Chrome 和本地模拟 API。运行 `node state/acceptance/browser-account-switch.mjs <解包目录>/dist/board-assets` 可核对随包页面并记录页面 JavaScript 的 SHA-256。[换号复验脚本](../state/acceptance/browser-account-switch.mjs)

## 0.3.15 候选

双平台归档由提交 `44a39b2` 构建。31 项冻结需求中 18 项通过、13 项待验，`releaseReady` 为 `false`。同版本的 69 项门槛自动检查全部通过。[逐项门槛报告](../state/acceptance/release-gate-0.3.15.json) · [现场证据索引](../state/acceptance/live-0.3.15.json)

| 项目 | 结果 |
| --- | --- |
| 保存和归档的账号绑定 | 两账号夹具使用同会话 ID、同位置和相同版本。旧账号表单无法保存，两个归档入口也拒绝旧账号的分区版本。新批次沿用旧账本的分区版本格式，旧批次中待核对项只回读，未提交项可核对后显式发送。 |
| Fedora 安装与回退 | 从已安装网页看板点击更新，重启依次读回 `0.3.14 → 0.3.15 → 0.3.14`。卸载恢复副本启动归档，持久数据夹具校验一致。[安装回执](../state/acceptance/fedora-installed-0.3.15.txt) |
| Fedora 已登录副本 | 本机和 MacBook Pro 专用卡均通过原窗口现有连接读回带账号标记的分区版本。远端专用卡以旧版分区版本提交同列保存时被拒绝，持久备注及三个版本均保持一致。跨账号测试使用夹具。[宿主回执](../state/acceptance/fedora-scope-0.3.15.json) |
| Fedora 标题与卡内控件 | 真实标题点击打开最近回复，调整按钮只打开编辑框，取消后卡片保持不变。可信标题拖动覆盖普通同列松手、拖出看板及右缘自动横滚，均无预览、原生跳转或保存。右缘横滚 290 像素并抵达最右列。入口为 Enhancer，原生 MCP App 仍待复验。[手势与控件回执](../state/acceptance/fedora-ui-gestures-0.3.15-1795366612671.jsonl) |
| Fedora 阻塞表单 | 实际拖入和在阻塞列同列拖放都打开表单。留空、纯空白、取消、填写和清空通过。旧保存被拒后，按账号隔离的空备注草稿保留。独立的输入及位置版本错误也被拒绝。原生位置、原标题和备注恢复，验收保存历史按正常规则保留。[表单回执](../state/acceptance/fedora-ui-002-0.3.15-1795366635072.jsonl) |
| 换号后的页面显示 | 编辑框打开时会暂停普通刷新，账号切换后旧草稿可能仍显示。已展开的归档结果也需要在换号时清除并重新读取。升级前的未保存草稿仍在浏览器旧键中，0.3.15 的编辑器需要有账号归属的草稿才会恢复。 |
| 双平台干净解包 | Fedora 通过 75 项随包 Node 测试、10 项公共入口测试及 18 项门槛测试。MacBook Pro 通过 73 项随包 Node 测试，跳过 2 项 Linux 专用测试，通过 10 项公共入口测试。两端各通过 10 项冻结模型检查。Mac Codex `26.1111.21308` 的 Framework 内嵌归档摘要，安装预检在复制前停止，系统归档校验一致。[Fedora 测试记录](../state/acceptance/fedora-clean-0.3.15.log) · [MacBook Pro 测试和预检](../state/acceptance/packages/macos-0.3.15-preflight.txt) |

## 0.3.14 候选

双平台归档由提交 `4c32dee` 构建。31 项冻结需求中 18 项通过、13 项待验，`releaseReady` 为 `false`。同版本的 68 项门槛自动检查全部通过。详见[逐项门槛报告](../state/acceptance/release-gate-0.3.14.json)和[现场证据索引](../state/acceptance/live-0.3.14.json)。

| 项目 | 结果 |
| --- | --- |
| 账号切换 | 夹具在两个账号使用相同会话 ID。切换后，旧的同列保存及排队 IPC 请求被拒绝，迟到预览无法返回旧账号回复，网页也无法读回旧账号的离线快照。 |
| Fedora 安装与回退 | 从已安装网页看板点击更新，重启依次读回 `0.3.13 → 0.3.14 → 0.3.13`。卸载还原副本启动归档，数据夹具通过 SHA-256 校验。[安装生命周期](../state/acceptance/fedora-installed-0.3.14.txt) |
| Fedora 本机及远程 | 已登录副本复用现有窗口连接。HTTP、安装包 CLI 和 MCP 读回本机及 MacBook Pro 专用卡的同一份状态、备注和版本。HTTP 与 MCP 预览返回相同原生轮次及回复，前后持久数据一致。MCP 列出冻结的 18 个共享操作及一个应用入口。[入口复验](../state/acceptance/fedora-entrypoints-0.3.14.json) |
| Fedora 连接诊断 | 临时限制为一页后，9 张远端卡只读，远端扫描和观察均未完成。Enhancer 看板的详情显示分页上限及自动状态未连接，原有状态、备注和版本保留。隔离配置逐字节恢复，重新读取后远端扫描完整。[原生窗口回执](../state/acceptance/fedora-health-0.3.14.json) |
| Fedora 标题拖动 | 在 Enhancer 真实窗口从专用卡标题起拖，分别在普通同列或看板外松手，以及在边缘自动滚动后松手。三次均无预览、原生跳转或保存；普通点击标题能打开预览。右缘横滚 290 像素并抵达最右列。专用卡和隔离配置读回一致。[手势回执](../state/acceptance/fedora-ui-gestures-0.3.14-1795366344155.jsonl) |
| MCP 全局入口 | 干净解包测试验证插件清单声明一个带图标的完整应用入口。MCP 与网页可以互相读回状态及备注，重启后仍可读回。通过原生最左侧图标打开的现场验收仍待补。 |
| 双平台干净解包 | Fedora 通过 73 项随包 Node 测试、10 项公共入口测试及 18 项门槛测试。MacBook Pro 通过 71 项随包 Node 测试，跳过 2 项 Linux 专用测试，通过 10 项公共入口测试。两端各通过 10 项冻结模型检查。Mac Codex `26.1111.21308` 内嵌 ASAR 摘要，安装器在复制前退出，系统启动归档 SHA-256 前后一致。[Fedora 测试记录](../state/acceptance/fedora-clean-0.3.14.log) · [MacBook Pro 测试和预检](../state/acceptance/packages/macos-0.3.14-preflight.txt) |

## 0.3.13 候选

双平台归档由提交 `e3af361` 构建。发布门槛逐项核对 31 条需求，18 条通过，13 条仍需现场凭证，`releaseReady` 为 `false`。新通过 `MANUAL-001`、`MCP-001`、`DATA-001`、`BOUNDARY-001` 和 `SECURITY-001`。见 `state/acceptance/release-gate-0.3.13.json`。

| 项目 | 结果 |
| --- | --- |
| 安装、更新、回退及卸载 | Fedora 从已安装看板点击更新，重启依次读回 `0.3.12 → 0.3.13 → 0.3.12`。卸载恢复副本启动归档，数据夹具 SHA-256 一致。见 `state/acceptance/fedora-installed-0.3.13.txt`。 |
| Fedora 本机与远程 | 已登录的隔离副本读回本机和 MacBook Pro 专用卡，两个主机均在线，两张卡均显示可打开。见 `state/acceptance/fedora-hosts-0.3.13.json`。 |
| Fedora 真实轮次 | 原生开始后进入进行中，手动停止后进入已阻塞。普通澄清答复结束后进入待审阅，预览读回同一轮最终答复。原生 Plan 表单等待输入时进入已阻塞，答复后同一轮恢复进行中。各阶段均保留人工备注。命令批准、系统错误和自动重试的现场复验仍待补。[开始与中断](../state/acceptance/fedora-interrupt-0.3.13-eca4ba7a.jsonl) · [澄清答复](../state/acceptance/fedora-clarification-0.3.13-d2646882.jsonl) · [等待输入](../state/acceptance/fedora-plan-input-0.3.13-41264236.jsonl) · [同轮恢复](../state/acceptance/fedora-plan-resume-0.3.13-47b0a4ca.jsonl) |
| Fedora 分页故障 | 将隔离配置限制为一页后，仍保留 10 张远程活动卡，其中 9 张暂为只读缓存。扫描及观察均报告未完成，原生打开缓存卡被拒绝。恢复配置后全部重新确认，专用卡备注保留。见 `state/acceptance/fedora-discovery-0.3.13.json`。 |
| Fedora 三种列模式 | 隔离看板的四列、五列及旧六状态模式中，列、顶部导航和编辑选项均符合上游顺序。当前本机和远程缺少「待判断」「已完成」原生分区，向这两列写入仍待验。配置逐字节恢复。见 `state/acceptance/fedora-state-001-0.3.13-1795362887379.json`。 |
| Fedora 阻塞表单 | 实际拖入阻塞列和在阻塞列内拖放均打开可留空的表单。取消保持原生位置和备注，纯空白保存为空字符串。过期状态、输入和分区版本分别被拒绝，空备注草稿在表单内保留。专用卡恢复位置、标题及备注，新增的验收变更历史按正常规则保留。见 `state/acceptance/fedora-ui-002-0.3.13-1795363338773.jsonl`。 |
| Fedora 搜索和刷新 | 标题和工作目录搜索均能定位专用卡，零结果时保留四列。手动及周期刷新后，待审阅列保持原滚动锚点和偏移。编辑表单打开期间跨过一轮刷新，看板未重绘。空列拖放、完整键盘操作及保存期间的刷新仍待复验。见 `state/acceptance/fedora-ui-003-0.3.13-1795363906942.jsonl`。 |
| Fedora 回复预览 | 安装版读回本机专用卡的同一轮澄清答复，格式无效的原生跳转请求被拒绝，卡片的状态版本、输入版本及分区版本保持一致。见 `state/acceptance/fedora-preview-0.3.13.json`。 |
| Fedora 远程标题 | 同一次人工保存完成标题同步，用时 510 毫秒。目标主机独立读回单一状态前缀以及完整表情、标签和正文。位置和备注保留，专用夹具的测试标题已恢复。见 `state/acceptance/fedora-title-0.3.13.json`。 |
| Fedora 网页与 PWA | 独立 Chrome 配置加载 0.3.13，Service Worker 获得页面控制，浏览器返回空的安装错误列表。系统安装及独立窗口启动仍待验。见 `state/acceptance/fedora-pwa-installability-0.3.13.json`。 |
| 持久化和迁移 | 已确认的人工保存和自动阶段与卡片数据一起写入变更记录。拒绝的请求和重复观察保持记录不变。迁移保留主库与远程分库的原始记录顺序，回退移除旧投影并保留后来写入的历史。 |
| 调用边界 | 发现、预览与改名复用现有窗口连接。发现读 `thread/list`、`project/list`、`thread/turns/list`，预览另用 `thread/read` 核对原会话，标题同步用 `thread/read` 和 `thread/name/set`。状态标记夹具验证不会发消息、发起轮次或执行真实归档。 |
| 双平台干净解包 | Fedora 通过 70 项随包 Node 测试、8 项公共入口测试及 18 项发布门槛测试。MacBook Pro 通过 68 项随包 Node 测试，跳过 2 项 Linux 专用测试。两端各通过 10 项冻结模型检查。macOS 系统 Codex `26.1111.21308` 的预检仍因内嵌 ASAR 摘要拒绝安装，系统启动归档 SHA-256 前后一致。见 `state/acceptance/fedora-clean-0.3.13.log` 和 `state/acceptance/packages/macos-0.3.13-preflight.txt`。 |

## 0.3.12 候选

双平台归档由提交 `0b22c31` 构建。`0.3.11` 的冻结模型夹具漏接了自动阶段持久化接口，两个对照断言失败。`0.3.12` 修复夹具，两端重新执行，10 项冻结模型检查全部通过。

| 项目 | 结果 |
| --- | --- |
| Fedora 真实中断 | 本机专用卡发起一轮后进入进行中，点击原生停止后进入已阻塞。目标分区独立读回，中断回执和历史轮次 ID 一致，自动原因、人工备注和标题均读回。隔离写入白名单复原。见 `state/acceptance/fedora-interrupt-0.3.12-7e923bad.jsonl`。 |
| Fedora 普通澄清答复 | 同一本机专用卡从已阻塞进入进行中，答复正常完成后进入待审阅。两次移动均由本机连接独立读回，预览返回同轮最终答复，人工备注保留。见 `state/acceptance/fedora-clarification-0.3.12-5f010661.jsonl`。 |
| 轮次故障夹具 | 验证同一阶段重启保留人工选择，新阶段可以移卡。原生写入后丢失回执再重启时，读回已生效的位置，保留备注且不重复写入。只读历史、排队输入、助手文字及请求自动重试不会单独触发自动移卡。 |
| 兼容服务身份 | 测试验证目录、发行包构建指纹、配置和端口匹配时才复用。在 Fedora 隔离副本中，CLI 复用已启动的服务，`stop` 关闭兼容端点，桌面进程继续运行。重启隔离副本后端点恢复，读回 `0.3.12`。见 `state/acceptance/fedora-service-stop-0.3.12.txt`。 |
| Fedora 安装生命周期 | 从已安装看板点击更新，重启读回 `0.3.10 → 0.3.12 → 0.3.10`。卸载恢复副本启动归档，数据夹具 SHA-256 一致。见 `state/acceptance/fedora-installed-0.3.12.txt`。 |
| 双平台干净解包 | Fedora 通过 61 项随包 Node 测试、6 项公共入口测试和 18 项发布门槛测试。MacBook Pro 通过 59 项随包 Node 测试，跳过 2 项 Linux 专用测试。Mac Codex `26.1111.21308` 的框架内嵌 ASAR 摘要，安装预检仍以退出码 `1` 停止。见 `state/acceptance/fedora-clean-0.3.12.log` 和 `state/acceptance/packages/macos-0.3.12-preflight.txt`。 |
| 统一发布门槛 | 31 项需求中 13 项通过，18 项待验。新通过 `AUTO-004` 和 `SERVICE-001`。`releaseReady` 为 `false`。见 `state/acceptance/release-gate-0.3.12.json`。 |

## 0.3.10 候选

双平台归档、校验文件及解包清单位于 `state/acceptance/packages/`。候选由提交 `446e10b` 构建。

| 项目 | 结果 |
| --- | --- |
| Fedora 分页与缓存 | 在隔离配置将远端扫描限制为每页 1 条、最多 1 页。10 张活动卡继续显示，其中 1 张为本次记录，9 张为只读缓存。缓存卡的原生打开请求被拒绝，未被标作已归档。恢复配置后 10 张卡都重新确认，专用卡备注保留。见 `state/acceptance/fedora-discovery-0.3.10-limit.json` 和 `state/acceptance/fedora-discovery-0.3.10-restored.json`。 |
| 历史和重连 | 夹具验证旧中断轮次保留人工选择、历史读取失败后轮换到其他卡、连接世代变更拒绝旧回执、恢复后重新获得操作权限。真实 MacBook 连接的重启可能触及用户共用的远端服务，实测保持现有远端连接。当前实测的来源正例只有 `vscode`，提供方正例只有 `openai`。 |
| Fedora 兼容服务 | `serve` 复用了现有实例。新版 CLI `stop` 校验实例后返回成功，HTTP 端点停止，隔离 Codex 进程仍运行。重启隔离副本后端点恢复，读回 `0.3.10`。见 `state/acceptance/fedora-service-stop-0.3.10.txt`。 |
| Fedora 安装生命周期 | 从实际已安装看板点击更新，重启依次读回 `0.3.9 → 0.3.10 → 0.3.9`。卸载恢复副本启动归档，数据夹具哈希一致。见 `state/acceptance/fedora-installed-0.3.10.txt`。 |
| 双平台干净解包 | Fedora 通过 53 项随包 Node 测试、6 项公共入口测试、18 项门槛测试。MacBook Pro 通过 51 项随包 Node 测试，跳过 2 项 Linux 专用测试。两端各通过 10 项冻结模型检查。Mac 系统构建的内嵌 ASAR 摘要仍阻止安装。 |
| 统一发布门槛 | 31 项需求中 11 项通过，20 项待验。新通过 `AUTO-006` 和 `DISCOVERY-002`。`releaseReady` 为 `false`。见 `state/acceptance/release-gate-0.3.10.json`。 |

## 0.3.9 候选

双平台归档、校验文件及解包清单位于 `state/acceptance/packages/`。候选由提交 `6bac065` 构建。

| 项目 | 结果 |
| --- | --- |
| Fedora 安装生命周期 | 从已安装看板点击更新，重启依次读回 `0.3.8 → 0.3.9 → 0.3.8`。卸载恢复副本的原始启动归档，数据夹具哈希保持一致。见 `state/acceptance/fedora-installed-0.3.9.txt`。 |
| Fedora 原生标题 | 在远程专用卡设置多个开头状态前缀，再从 `0.3.9` 保存。当前窗口的远端连接独立读回结果。只有开头前缀替换成 `[待审阅]`，长标题、正文标签和备注均保留，卡片版本只加一。见 `state/acceptance/fedora-title-0.3.9-native.json` 和 `state/acceptance/fedora-title-0.3.9-save.json`。 |
| 标题故障夹具 | 挂起的宿主上下文按三秒预算返回。前台保存可在旧标题任务等待回执时完成。重开后读回远端已经写入的标题，持续失败项让其他待同步卡先尝试。首次启用时间在重开后保持，补读此前的完成轮次没有触发改名。 |
| Fedora 最左侧 MCP 图标 | `26.1111.71704` 已发现随包原生全局入口。当前登录方式会将置顶写入同一 ChatGPT 账号设置。已验证插件详情入口，最左侧置顶和点击需使用独立账号验收，或获准修改该账号的侧栏设置。 |
| 双平台干净解包 | Fedora 通过 45 项随包 Node 测试、6 项公共入口测试和 18 项门槛测试。MacBook Pro 通过 43 项随包 Node 测试，跳过 2 项 Linux 专用测试。两端各通过 10 项冻结模型检查。Mac 的系统 Codex 仍在安装预检被拒绝。见 `state/acceptance/fedora-clean-0.3.9.log` 和 `state/acceptance/packages/macos-0.3.9-preflight.txt`。 |
| 统一发布门槛 | 31 项需求中 9 项通过，22 项待验。`DELIVERY-001` 通过的是双平台发行归档检查，真实 Mac 桌面运行仍待验。`releaseReady` 为 `false`。见 `state/acceptance/release-gate-0.3.9.json`。 |

## 0.3.8 候选

双平台归档、SHA-256 校验文件及解包清单位于 `state/acceptance/packages/`。当前候选由提交 `f1b2099` 构建。

| 项目 | 结果 |
| --- | --- |
| Fedora 安装生命周期 | 从已安装看板点击更新，重启依次读回 `0.3.7 → 0.3.8 → 0.3.7`。卸载恢复副本的原始启动归档，数据夹具哈希保持一致。见 `state/acceptance/fedora-installed-0.3.8.txt`。 |
| Fedora 原生 MCP App | 从已安装插件打开 MCP App，将远程专用卡拖到进行中，目标主机独立读回新分区。刷新后卡片及备注保留。网页先保存时，MCP App 拒绝旧表单，保留未提交的草稿，独立读回只保留较新的已保存备注。见 `state/acceptance/live-0.3.8.json` 的原始附件。 |
| Fedora 浏览器离线恢复 | 停止 `0.3.7` 隔离宿主后，Service Worker 返回 `503` 离线页，页面只提供「重新连接」。启动 `0.3.8` 后点击「重新连接」，同一 Chrome 配置重新显示看板，读回版本 `0.3.8`。系统 PWA 安装及独立窗口模式仍待验。 |
| MacBook Pro 真机 | `0.3.8` 的归档及源码校验完成，37 项 Node 测试通过，跳过 2 项 Linux 专用测试。两端各通过 10 项冻结模型检查。Mac 安装预检在复制前退出，系统启动归档哈希保持一致。见 `state/acceptance/packages/macos-0.3.8-preflight.txt`。 |
| macOS 兼容构建复查 | 12 个完成检查的官方签名 arm64 构建中，10 个有框架内嵌 ASAR 摘要，另两个缺少当前接入使用的界面、账号及侧栏绑定。见 `state/acceptance/packages/macos-official-builds.md`。 |
| 统一发布门槛 | Fedora 干净解包通过 39 项 Node 测试。门槛读入 8 条通过的现场断言，31 项需求中 `AUTO-005`、`DATA-002` 及 `GATE-001` 通过，另外 28 项待验。退出码为 `2`，`releaseReady` 为 `false`。见 `state/acceptance/release-gate-0.3.8.json`。 |

## Fedora 44 实测

宿主是 Codex `26.1111.71704`。远程测试使用同一窗口已经连接的 `remote-ssh-discovered:macbook`。涉及会话写入的操作只使用本轮创建的专用卡。

| 操作 | 实测结果 |
| --- | --- |
| 本机及远程原生拖放 | 两张专用卡从待审阅拖到进行中，原生侧栏显示变化，目标主机读回新分区。从原生侧栏移回后，旧表单被拒绝，已保存的备注保留。见[移卡检查点](session-board-native-checkpoint.md)。 |
| 发现和预览 | 已连接主机返回 9 个远程项目。两端读回的最新完整回复分别为 `01a2030f-d1d1-78c2-b969-9ae91b6fd013` 和 `01a2030f-d1ea-78b0-b061-713742857e5c`，均核对了专用回复标记。 |
| 观察器 | 关闭看板后，在远程专用会话发起新一轮请求。看板读回「进行中」及「待审阅」。修正轮次条目分批到达导致的重复计数后复验，新轮次 `01a2034e-2fb6-75e2-9e3c-13bbe81178aa` 仅将输入版本从 2 增至 3，标题读回且备注保留。 |
| 手动归档和重读 | 本机批次 `4ac26c51-d1b3-494e-9c29-431ae248c999` 由 `awaiting` 到 `archived`。重启后卡片和批次仍可重读，备注保留。进入归档记录时，原生页面定位到相同 UUID 和 `hostId=local`。 |
| 远程定时归档 | 临时配置远程间隔为 12 秒且保留专用会话写入限制。关闭看板后将远程专用卡置于待归档，后台批次读回归档且备注保留。结束后恢复测试配置，从归档记录进入原主机并在原生窗口解除归档，再读回活动卡。 |
| 项目新建入口 | `0.3.7` 打开本机默认工作区的空白原生输入页，本机会话 ID 集合保持一致。远程专用目录在原生窗口完成登记后，从看板打开同一项目的空白输入页。发送前目标目录没有会话，手动发出首条测试输入后目标主机读回一条新会话。首次 `projectId` 为空，走看板的显式补登记操作后，目标主机读回规范项目 `01a2037c-8f2c-73a1-b0a7-173b85d33e11`。原始记录位于 `state/acceptance/fedora-remote-composer-0.3.7.jsonl`。 |
| 网页和 PWA | 隔离 Chrome `158.0.8127.36` 加载实际看板，Service Worker 获得控制，Chrome 报告无可安装性错误。系统安装和独立窗口启动仍待签收。 |
| CLI 与 MCP | CLI `serve` 复用实际窗口服务，`scan` 读回本机、1 台远程主机和 9 个项目。MCP stdio 通过 `2025-11-25` 握手并列出 18 个共享操作和 1 个应用入口。隔离副本从本地市场安装 `0.3.7`，经 Codex 插件详情的「打开插件」渲染原生 MCP App。在 App 中修改远程专用卡备注，备用网页 API 及 Enhancer 侧栏都读回同一版本的备注。原始记录位于 `state/acceptance/fedora-native-mcp-0.3.7.jsonl`。 |
| 分区预检及服务停止 | `check-sections` 因本机缺「已阻塞」以 `1` 退出，远程健康快照仍显示四个独立原生 ID。本机专用卡移入「已阻塞」后，同一命令以 `0` 退出，输出的新分区 ID 与移卡读回一致。`stop` 关闭网页和 CLI 服务而桌面进程继续运行，断开后 `scan` 提示恢复宿主。 |
| `0.3.7` 登录后复验 | 从实际网页 API 分别移动两端专用卡，目标主机回读一致，备注保留，标题同步读回。归档本机专用卡后重启，归档及排序均保留，相同版本的重复排序被拒绝。随包 CLI `serve` 复用该窗口，MCP 实际读回相同排序。原始记录位于 `state/acceptance/fedora-signed-0.3.7.jsonl`。 |

复验入口分别为[预览及定时归档](../acceptance/fedora-tail.mjs)、[轮次观察](../acceptance/fedora-observer.mjs)与 [PWA 可安装性](../acceptance/verify-pwa.mjs)。其中定时归档和轮次观察只适用于尚未归档的指定专用夹具。

## 安装、更新和回退

Fedora 安装版从实际网页看板点击「更新」。`0.3.5 → 0.3.7 → 0.3.5` 的每次重启均读回对应版本。隔离数据夹具的 SHA-256 保持一致，卸载恢复了副本的原始启动归档。原始记录位于 `state/acceptance/fedora-installed-0.3.7.txt`。用当前安装器从旧 Enhancer 启动器升级后，原登录配置的测试副本加载了 `0.3.7`。

MacBook Pro 上的 macOS arm64 和 Codex `26.1111.21308` 已单独取证。实际安装检查发现 `Codex Framework` 内嵌 ASAR 完整性字典摘要。隔离副本的签名检查通过后，桌面启动仍因完整性摘要拒绝。`0.3.7` 在真机复验时通过归档校验，安装器在复制前以退出码 `1` 报告该条件，系统归档 SHA-256 前后一致。证据位于 `state/acceptance/packages/macos-0.3.7-preflight.txt`。早期版本已在 Mac 上验证解压、版本选中和回退，桌面运行仍待验。

## 自动对照与验收门槛

- 冻结的 31 条需求及独立模型以 SHA-256 校验。从指定提交独立导出的 Rust `board_contract` 和 `board_io` 共 86 项通过。Rust 与 TypeScript 对同一模型分别重放 520 条边和 1,417 次操作，比较状态、人工选择、备注及版本。其中 40 次重开和 40 次查看分别核对了持久数据未变化。更广的 Rust 形式模型及旧新回执逐项比较仍待补齐。
- 干净解包的 `0.3.7` 在 Fedora 通过 38 项测试。MacBook Pro 通过 36 项，跳过 Linux 安装包头和热回滚日志夹具两项。两端均通过 10 项冻结模型检查，运行代码指纹一致。
- 归档包含运行时、MCP 资源、源码和编译后的测试、冻结需求及绑定。构建时排除运行目录、日志、缓存及 `node_modules`。
- [统一发布门槛](../acceptance/release-gate.md)将 31 项需求逐条绑定到测试和现场附件，核对候选源码、双平台归档、运行文件、时间和范围。15 项门槛回归覆盖缺失、过期、失败、跳过、变更、范围错配和通过条件。
- [逐项清单](../acceptance/candidate-map.json)保留未验断言。尚需独立验证同 ID 跨主机的真实写入、原生 MCP App 的最左侧直达图标及拖放、系统 PWA 安装、macOS 宿主运行，以及故障和重连场景。`releaseReady` 仍为 `false`。
