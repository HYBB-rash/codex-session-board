# 远程原生新建

看板打开 Codex 原生草稿，用户发送第一条输入时由原生创建会话。
客户端适配不修改 app-server、clientInfo 或已有会话的 originator。
v2 包含两条不同的入口，不能交换它们的项目 ID：

- `codex://threads/new-remote?hostId=...&projectId=...` 沿用操作机登记的远程项目；
  原生重新检查 `getRemoteProjects()`，调用既有项目选择及导航。
- `codex://threads/new-target?hostId=...&projectId=...&operationId=...` 接受目标后台的
  canonical project ID 和稳定操作 UUID。只接受这三个参数，不接受路径、roots 或 prompt。
  `targetServer` 直接使用后台 ID；`targetLegacy` 在用户明确新建时完成幂等项目导入后，
  使用返回的 canonical ID。项目准备不导入 threads，不补 metadata 别名，也不写远端 legacy state。

目标入口不更改全局项目选择，也不要求目标 ID 出现在操作机项目登记表中。
每个 operationId 对应独立的原生 client draft ID；重复打开保留原草稿，不覆盖其他草稿。
原生连接重新读取 `project/read`，保留有序的全部 roots，并采用首个 root 为 cwd。
已有绑定的操作不会重新创建会话；失败提示保留已知会话 UUID。

当前 v2 固定 Mac 26.1111.21308 和当前 Fedora 来源的主进程、deeplink 及七个 renderer 模块摘要。
26.1106 的已核对入口属于历史 v1，不能因此声明支持 v2；未知来源拒绝构建。

## 原生提交链

路由接收器注册目标描述符和原生草稿身份，`V3rO` 按草稿查找目标。
`HSUP` 使用目标主机和工作目录，加载或失败状态显示在原生输入框上方。
`aVy5` 使用目标 workspace 分支，跳过操作机项目选择和 localProjectId 覆盖；
`e1yv`、`QX7Q`、`_D` 保留描述符，`TSah.readCreationInputs` 把 canonical ID 放入
本次调用的 `inputs.projectId`，其后使用原有 `xz.prepareConversation/prepareStart`。
旧 `projectAssignment` 保持 undefined，不能传 null，也不调用旧项目归属补写。

目标草稿关闭 `cojq` 中预热调度器的输入标记；这处 `submitDisabled` 只属于预热函数的参数，
不改变发送按钮的禁用条件。原有 builder 在 projectId 非空时也不能消费通用预热。
源函数回归实际调用 `aVy5` 的提交方法到原生 dispatch，另以原函数证明通用预热原本可用、
候选目标不可用，防止把关闭预热误写成关闭发送。

首次读取绑定原生账户和用户，读取前后、创建前及首轮发送前检查账户是否变化，
并验证 canonical metadata 的活动 backing/creation owner。认证加载、owner 不符、
roots 改变或客户端已知的 pending deletion 会拒绝发送，保留草稿并要求明确重开。
pending deletion 读取原生同步全局状态中该 host 的 canonical ID；目标另一客户端尚未暴露
给本客户端的暂存删除意图不能靠 project/read 观察。它不是跨客户端原子锁。
发送前检查与发送之间仍存在并发窗口。首轮接受后后台可能尚未发布持久化归属，
因此只对同一 UUID 的空 projectId 做有限读回等待；出现其他项目则立即拒绝，
等待结束仍为空时报告“尚未确认”和原 UUID，不自动换 ID 重建。

## 准备客户端候选

开发工具只生成新 archive 和能力清单，不覆盖来源，不安装或重启客户端：

```zsh
python3 scripts/native_new/prepare.py /path/to/source/app.asar \
  /path/to/candidate.app/Contents/Resources/app.asar \
  --open-command '["open","-a","/path/to/candidate.app"]'
```

目标 archive 必须尚不存在，启动命令必须明确指向包含它的客户端。
不能通过系统默认协议启动器声明另一个候选的能力。
单独生成 archive 后，集成方仍须更新 Electron 完整性元数据和应用签名。
macOS 可以准备一份完整的本地候选，保留原应用：

```zsh
python3 scripts/native_new/prepare_app.py /Applications/ChatGPT.app \
  /path/to/Codex-remote-native.app
```

该命令复制应用、更新 archive 完整性元数据并做本地 ad-hoc 签名和静态验签。
它不启动、安装或替换日用客户端，也不证明登录、原生页面及首轮执行已通过。
客户端升级后重新生成；不匹配的内部入口会明确拒绝构建。

配置 native_new_capability 指向生成的 app.remote-new.json，
open_command 必须与能力清单中的 openCommand 一致。
目标入口要求 `codex-remote-draft-v2`，看板在每次远程新建前核对协议、启动命令与实际 archive 摘要。
清单缺失、摘要变化或启动目标不同都拒绝远程新建，提示更新原生入口。
静态能力核对不证明正在运行的旧进程已经重启，正式交付必须另做真实验证。

## 验证

自动验证覆盖链接参数、操作身份、账户和待删除状态、远程目标、原包不变及 archive 完整性。
运行时回归并入 `node scripts/check-native-new.mjs`。对实际来源执行候选原函数验证：

```zsh
python3 -B scripts/native_new/verify_sources.py /path/to/source/app.asar
```

工具也接受包含原包内部路径的只读导出目录，适合 Fedora 跨平台源码验证。
它在内存生成候选，执行真实 `V3rO/HSUP/aVy5/e1yv/QX7Q/_D/TSah/xz.prepareStart`，
用两个共享 cwd/roots 的项目和未改函数作对照，不创建 archive，不发送真实请求。
这些函数验证不是 GUI 验收，也不代替后台首轮和数据库读回。
需要同时验证首轮和落库时，显式指定本机原生后台可执行文件：

```zsh
python3 -B scripts/native_new/verify_sources.py /path/to/source/app.asar \
  --backend /absolute/path/to/codex
```

该选项为新进程创建独立 HOME、CODEX_HOME 和工作目录，模型只连接本地 SSE 替身。
它执行生成候选的 `_D/xz.prepareConversation/prepareStart` 和目标运行时，
在 turn/start 刚接受时立即调用归属核对，再让两个同目录项目分别完成首轮，
再核对 thread/read 与 SQLite 的 project_id、cwd，最后清理进程和临时目录。
通用配置读取由替身提供；本探针验证目标参数和原生落库，不证明登录和 GUI 可用。
用 Fedora 导出源在 Mac 上执行时，证明的是 Fedora 客户端函数与该 Mac 后台的组合，
不能据此宣称 Fedora 在线环境验收完成。

实际安装候选后，须在专用窗口核对远程项目和输入框，不覆盖已有草稿。
首轮实测必须核对同一远程主机、项目、cwd，以及原生 Codex 的来源标识。
当前客户端默认来源为 Codex Desktop，initialize 的连接名为 codex_desktop。
还需检查多窗口、未连接主机、项目删除、重复点击和客户端升级后的拒绝行为。
缺少这些目标环境证据时完整发布继续保持待验证。

## 共享 daemon 的迁移

Codex 0.167.1 和 0.169.0 的隔离双连接实验确认，未设置原生默认来源的共享 app-server
会把第一个 initialize 的客户端名用于后续新会话，即使创建来自另一连接。
因此旧 daemon 若先由看板初始化，单改创建入口后仍可能显示旧来源。
部署时需要在确认远端没有运行任务后，让 Codex 原生重新启动并初始化 daemon，
再恢复看板连接；不能只重启桌面界面，也不能直接改旧会话的来源记录。
原生客户端启动环境中的默认来源具有优先级，验收需核对实际来源而非仅看连接名。
可运行独立协议探针，不发送模型输入：

```zsh
node scripts/native_new/verify_originator.mjs /path/to/codex
```

探针只证明当前 CLI 的共享进程归属行为，不代替真实远程首轮。
上述实验是独立协议证据，不用于选择或认领当前客户端后台。
业务连接继续经过 App::connection；端点继承、归属校验和重连由连接层处理，
不从 daemon version 的 socketPath 推断后台。Mac 入口见 SHARED-DESKTOP.md。
