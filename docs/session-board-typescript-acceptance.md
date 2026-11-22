# TypeScript 会话看板验收

上游行为基线为 `2cc4e6bb4e03d7cfabce0733a91a328004a9f3fc`。Fedora 44 的 Enhancer 副本通过本机和远程原生移卡检查点。macOS arm64 的目标 Codex 构建在加载看板之前拒绝了经过修改的启动归档，因此发布条件未满足。

## 0.3.13 候选

双平台归档由提交 `e3af361` 构建。发布门槛逐项核对 31 条需求，18 条通过，13 条仍需现场凭证，`releaseReady` 为 `false`。新通过 `MANUAL-001`、`MCP-001`、`DATA-001`、`BOUNDARY-001` 和 `SECURITY-001`。见 `state/acceptance/release-gate-0.3.13.json`。

| 项目 | 结果 |
| --- | --- |
| 安装、更新、回退及卸载 | Fedora 从已安装看板点击更新，重启依次读回 `0.3.12 → 0.3.13 → 0.3.12`。卸载恢复副本启动归档，数据夹具 SHA-256 一致。见 `state/acceptance/fedora-installed-0.3.13.txt`。 |
| Fedora 本机与远程 | 已登录的隔离副本读回本机和 MacBook Pro 专用卡，两个主机均在线，两张卡均显示可打开。见 `state/acceptance/fedora-hosts-0.3.13.json`。 |
| Fedora 三种列模式 | 隔离看板的四列、五列及旧六状态模式中，列、顶部导航和编辑选项均符合上游顺序。当前本机和远程缺少「待判断」「已完成」原生分区，向这两列写入仍待验。配置逐字节恢复。见 `state/acceptance/fedora-state-001-0.3.13-1795362887379.json`。 |
| Fedora 阻塞表单 | 实际拖入阻塞列和在阻塞列内拖放均打开可留空的表单。取消保持原生位置和备注，纯空白保存为空字符串。过期状态、输入和分区版本分别被拒绝，空备注草稿在表单内保留。专用卡恢复位置、标题及备注，新增的验收变更历史按正常规则保留。见 `state/acceptance/fedora-ui-002-0.3.13-1795363338773.jsonl`。 |
| Fedora 回复预览 | 安装版读回本机专用卡的同一轮澄清答复，格式无效的原生跳转请求被拒绝，卡片的状态版本、输入版本及分区版本保持一致。见 `state/acceptance/fedora-preview-0.3.13.json`。 |
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
