# TypeScript 会话看板验收

上游行为基线为 `2cc4e6bb4e03d7cfabce0733a91a328004a9f3fc`。Fedora 44 的 Enhancer 副本通过本机和远程原生移卡检查点。macOS arm64 的目标 Codex 构建在加载看板之前拒绝了经过修改的启动归档，因此发布条件未满足。

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
