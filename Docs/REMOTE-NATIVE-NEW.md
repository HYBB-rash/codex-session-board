# 远程原生新建

看板只向客户端传递已登记远程主机和项目。
客户端调用原生项目选择和新建页面导航，用户发送首条输入时由 Codex 创建会话。
看板不向远端发送 thread/start，不修改 clientInfo 或已有会话的 originator。

当前核对的客户端为 ChatGPT.app 内的 Codex 26.1106.51428。
原有 codex://new 项目查找只支持本地项目，create_thread 工具要求非空 prompt。
因此增加独立入口 codex://threads/new-remote?hostId=...&projectId=...。
入口只接受这两个参数，原生侧重新核对远程登记表，再调用既有选择和导航函数。
项目选择对象沿用原生的 type/projectId 格式，主机写入独立的原生主机选择状态；
两者一起广播，再导航到新建页。跨主机重复的原生项目 ID 拒绝导航。
项目消失、主机不符或目录异常时拒绝，不自动创建项目。

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
看板在每次远程新建前核对协议、启动命令与实际 archive 摘要。
清单缺失、摘要变化或启动目标不同都拒绝远程新建，提示更新原生入口。
静态能力核对不证明正在运行的旧进程已经重启，正式交付必须另做真实验证。

## 验证

自动验证覆盖链接参数、远程目标、选择与导航顺序、原包不变及 archive 完整性。
实际安装候选后，须在专用窗口核对远程项目和输入框，不覆盖已有草稿。
首轮实测必须核对同一远程主机、项目、cwd，以及原生 Codex 的来源标识。
当前客户端默认来源为 Codex Desktop，initialize 的连接名为 codex_desktop。
还需检查多窗口、未连接主机、项目删除、重复点击和客户端升级后的拒绝行为。
缺少这些目标环境证据时完整发布继续保持待验证。

## 共享 daemon 的迁移

Codex 0.167.1 的隔离双连接实验确认，未设置原生默认来源的共享 app-server
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
上述 0.167.1 实验为独立历史协议证据，不用于选择或认领当前客户端后台。
业务连接继续经过 App::connection；端点继承、归属校验和重连由连接层处理，
不从 daemon version 的 socketPath 推断后台。Mac 入口见 SHARED-DESKTOP.md。
