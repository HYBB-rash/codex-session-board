# 拖动后的龙娘反馈

2026-11-17 用户确认白发紫眸、白紫礼服、分节龙角、龙翼和长尾的
候选造型，授权制作动画并接入。美术使用内置 imagegen 根据已确认的
候选图重新生成，没有采用外部作者的 PNG/GIF 图集。

## 素材与动作

唯一运行素材在 `index.html` 的 `move-dragon` 图片中，以 WebP data URI
内嵌。768×512 的透明图集分为 4 列 2 行，上排抱卡等待、眨眼与摆尾，
下排勾选卡片、举手庆祝。CSS 切帧，成功动作结束后保持最后一帧。
减少动态效果时，两种状态都显示各自的静态画面。
HTTP、MCP、隔离浏览器和源码包读取同一 HTML，无外部图片请求。

内置 imagegen 生成 1536×1024 透明 PNG，再使用 FFmpeg 等比缩放并编码为
WebP。只转换尺寸和编码，保留透明通道。为避免重复维护图片与嵌入内容，
仓库不另存同一运行图集。可以从图片 src 的 data URI 提取 WebP。

最终图集生成提示词如下。输入图为用户已确认的双状态候选图。

```text
Use case: identity-preserve / animation sprite sheet. Image 1 is the APPROVED character identity and visual style reference. Create a production-ready TRANSPARENT PNG sprite sheet of exactly this SAME white-haired lavender-eyed chibi dragon girl in white and pale lavender dress with segmented curved white horns, lavender-white wings, ribbon flower/knot hair ornament, long white scaled tail. Preserve her face, silhouette, hair, costume and palette. All characters are full body, modest, same proportions as reference. Clean cel-shaded anime art with clearly visible linework at small UI size.
Layout MUST be a perfectly regular 4 COLUMNS x 2 ROWS grid of EIGHT equally sized sprite cells, no borders or labels. Prefer landscape 1536x1024 canvas, each equal cell 384x512. Every pose uses identical camera, same character scale, feet at 92% cell height, face at same approximate pixel position; center of torso in cell center. Entire wings, tail, horns, toes within the cell with generous transparent padding on all sides. Transparent pixels between cells and around every pose, NO checkerboard rendered into art, NO background, NO shadow ellipse, NO written words, NO UI, NO framing, NO arrows.
Top row: four frames of a gentle looping WAITING animation, girl holds the small kanban card with three tiny lavender pending dots, no checkmark, patient expression. Frame1 eyes open tail slightly left wings relaxed; frame2 slight blink and tail centered; frame3 eyes softly closed for a blink tail slightly right; frame4 eyes open tail returning left. Whole body stays aligned, only very small breathing movement, hair and wings gently change. Avoid jumps in design or scale.
Bottom row: four frames of SUCCESS celebration animation, same girl smiling, eyes softly closed, holds same card now bearing a CLEAR GREEN CHECKMARK, free hand raised in a small celebratory fist. Frame1 start hand lift tail relaxed; frame2 hand raised tail curling up wings slightly raised; frame3 hand fully raised happy small bounce tail curled and 2 tiny golden sparkles; frame4 settle back to happy raised-hand pose and tail returning. Success row also same fixed scale and alignment as waiting. NO success symbols in waiting row. This is sprite artwork only for CSS steps animation; exact regular grid and consistent character identity are critical.
```

## 展示与保存

保存仍立即发出，沿用 1500ms 后出现角色的策略。角色解码完成且页面在前台时，
才记录实际展示时间。快速保存不强制出现角色。

收到成功回执立即更新真实状态、解除保存锁。成功画面从前台可见时起保留
2000ms，包含成功动画与最后的静止画面。无须先等待动画才允许后续操作。
这样也满足角色实际出现后至少可观看 2 秒。尚未收到回执时持续显示等待态。
后台时间不计入成功画面的观看时间，恢复前台后补足剩余时间。

新操作接管当前反馈，旧操作回调先核对归属。手动收起立即隐藏并清理计时器，
慢响应和素材解码回调不会把已收起的旧角色重新展开。
失败或结果未确认时保留未确认文案及等待行的静态角色，不播放成功动画。
素材解码失败时退回真实状态文字，保存逻辑继续运行。

## 回归入口

`scripts/check-transfer-feedback.mjs` 使用回环 HTTP fixture、独立浏览器 profile
和随机调试端口，不连接日用 Codex。脚本输出精确时钟断言、两态逐帧截图、
窄屏及减少动态效果截图和浏览器清理结果。现有 Rust UI 测试运行这组场景，
HTTP 与 MCP 测试检查实际产出的页面包含内嵌素材。

隔离证据不代替目标 Codex 原生容器验收，不表示已安装、重载或部署。

真实 Rust fixture 演练使用 `scripts/check-dragon-fixture.mjs`，先启动本工作树的
`scripts/dev`，再传入新的源码外报告目录。它核对服务身份和具体可写卡片，
通过真实指针拖动、请求放行与服务端读回验证保存，用真实时钟测量成功态，
输出窄屏、动画帧和清理证据。停止后重新启动同一 fixture，再传入同一目录和
`--verify-only`，核对保存状态与备注仍然存在。示例：

```sh
./scripts/dev setup
./scripts/dev start
node scripts/check-dragon-fixture.mjs /absolute/new-fixture-report
./scripts/dev stop
./scripts/dev start
node scripts/check-dragon-fixture.mjs /absolute/new-fixture-report --verify-only
./scripts/dev stop
```

320×568 竖屏回归包含纵向滚动条，提示面板按固定包含块的可用宽度收缩，
避免 `100vw` 包含滚动条宽度时越过左边界。

保存成功回执直接携带已确认的 `sectionVersion`。前端在恢复拖动前应用它，
因此连续拖动不依赖后续列表刷新，也不会用上一位置向原生服务再次写入。
