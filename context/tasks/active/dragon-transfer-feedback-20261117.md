# 拖动后的 GPT 龙娘反馈

owner: codex
execution_mode: sequential
allowed_write_set: index.html, Docs/DRAGON-FEEDBACK.md, scripts/check-transfer-feedback.mjs, tests/ui_layout.rs, tests/board_io.rs, context/tasks/active/dragon-transfer-feedback-20261117.md, context/tasks/completed/dragon-transfer-feedback-20261117.md, context/handoffs/dragon-transfer-feedback-20261117.md, context/journal/2026-11-17-dragon-transfer-feedback.md
base_commit: 4188175fd9468092fb9ca511c116f6681faa6e97
worktree_path: /home/herman/文档/notion-program
coordinator: 当前会话

## 目标与边界

将默认搬运工替换为用户确认的 GPT 龙娘。角色实际可见后至少展示
2 秒，仍在等待保存时继续展示；保存成功后的成功画面保留 2 秒。
保存不等待动画，后续操作不被动画额外阻止，旧计时器不得隐藏新反馈。
手动收起立即生效，失败或未确认不得显示成功；保留窄屏及减少动态效果。
使用隔离 fixture，不安装、重载日用插件，不修改官方客户端，不推送或部署。
开发不改版本清单和共享 STATE。

## 2026-11-17 角色确认点

开始时工作区干净。已阅读上下文、验收契约、迁移说明及原反馈任务和交接。
仓库检索没有找到龙娘素材或已确认的角色记录。
现实现等待 1500ms 才展示，finishMove 成功后 900ms 隐藏；
scripts/check-transfer-feedback.mjs 已提供可控时钟及浏览器交互验证。

联网发现白发、龙角、龙翼、长尾、浅色礼服的 GPT 社区形象参考。
原作者 B 站页面返回验证码，未直接核对原帖图片。
现成 Pet 图集仓库说明角色和 PNG/GIF 美术不在 MIT 授权范围，
不直接将其图集当作项目可分发资产。

参考线索：

- https://github.com/HABAI182/ai-girls-atlas/blob/main/characters/gpt/README.md
- https://www.bilibili.com/opus/1233069202138464289
- https://github.com/tqlearning/ai-girl-chatgpt-pets

使用内置 imagegen 生成白发紫眸、白紫衣裙、分节龙角、翅膀和长尾的
双状态候选静态图。左侧抱卡等待，右侧持成功勾选卡庆祝。
该图仅供造型确认，不是已接入的动画，也不代表用户已经确认指定形象。

预览保留在源码之外：
/home/herman/.codex/generated_images/01a1e91e-eee2-76a0-87b1-81287595ff28/call_YsnLX2KfqBeZpmSpa2tUVBcI.png

用户随后明确确认造型，授权制作动画、接入看板、完成计时与隔离验证。
当前已制作同一角色的 4 列 2 行透明图集，计划作为 WebP data URI 内嵌页面，
保持 HTTP、MCP 和直接读取 HTML 的 fixture 使用同一资源，无外部资源请求。
仅转换尺寸和编码，不直接使用外部作者的美术图集。
保存与动画分别收尾，素材解码、前后台切换和旧操作计时器归属纳入回归。

## 实现检查点

已替换默认角色、两态动作与文案，内嵌 159478 字节透明 WebP。
保存仍立即发出；成功态从实际可见起保留 2000ms，后台时间暂停扣减。
每个显示和隐藏回调检查操作归属，手动收起立即清理等待和收尾计时器。
失败态持续提示未确认，不显示成功帧。保留 1500ms 慢反馈出现策略。

隔离浏览器首轮 62 项通过，含真实指针拖动、图集解码、2 秒边界、连续拖动、
保存锁即时释放、手动收起、失败、320/736/1680px、减少动态效果、
后台完成与暂停续时、慢解码。三项定向 Rust 包装、HTTP/MCP 资源检查通过。
两名只读审阅者分别检查计时逻辑和素材/CSS 接入，没有阻断发现。
后续运行统一门禁、真实 Rust 开发 fixture 并记录清理。
证据根目录：
/home/herman/.codex/visualizations/2026/11/17/01a1e91e-eee2-76a0-87b1-81287595ff28
