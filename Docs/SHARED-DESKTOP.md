# Mac 客户端共享后台

看板的实时连接只认启动本插件的客户端 App Server 进程。
Unix 连接的对端 PID 和启动时间由系统核对，连接和后续请求都检查归属。
同一个 CODEX_HOME、版本相同或 socket 权限正确，都不能证明属于当前客户端。
运行中的看板也按客户端后台归属隔离，其他客户端不能复用它。

## 启动入口

Mac 使用原装客户端支持的 CODEX_APP_SERVER_WS_URL，将 GUI 和后台接到同一个
Unix WebSocket。启动器在创建后台之前设置该环境，让后台启动的插件自然继承。
看板自动采用继承的端点，不再要求 Agent 从 daemon version 输出猜测地址。
配置里的旧 desktop_socket 不会覆盖客户端实际继承的连接。

```zsh
python3 scripts/shared_desktop.py --probe --report /private/tmp/csb-mac-sharing.json
```

此命令自动创建私有 CODEX_HOME 和独立 GUI profile，启动客户端自带的后台，
核对真实客户端 WebSocket 握手、版本、后台 PID，并关闭自己创建的两个进程。
不使用账号凭据，不创建聊天或执行模型任务。日志与报告留在源码之外。
客户端退回 stdio、被已有单实例接管、版本不符、后台退出或端点被替换都会失败。
这是 Mac 真实传输检查，不代替完整 31 条业务验收。

切换日用启动方式属于单独部署。先退出使用目标 profile 的客户端，再使用同一入口：

```zsh
python3 scripts/shared_desktop.py --home /absolute/codex-home --profile /absolute/client-profile
```

home 和 profile 必须是已存在且属于目标客户端的目录。可用 --app 指定应用包，
--backend 指定已验证的兼容后台。启动器在前台持有这两个子进程；退出时只清理自己
创建的进程。每次启动使用新私有 socket，不接管旧守护进程、不扫描并尝试任意后台。
首次部署需要为目标客户端配置一次该入口；后续业务 Agent 只使用 App::connection。
直接启动的 stdio 客户端没有共享端点时，看板如实报告实时连接不可用并保留状态。

## 自动门禁

编译阶段拒绝业务模块直接建立 DesktopConnection 的原始连接；
统一入口服务于观察、标题、新建、分区和归档。远程继续使用已登记主机的连接校验。
自动验收绑定错误后台的六类角色、过期启动时间、未加载历史投影和启动器失败场景。
隔离开发 fixture 由同一模拟后台启动看板，遵循相同的父进程与对端核对。

只有已加载的 active / idle 或 systemError 参与实时状态推断。notLoaded 和未知
状态不将历史投影的 interrupted 认定为真实暂停，历史补读仍走独立读取接口。
连接错误不修改人工选择，也不会批量重置已经保存的卡片。

2026-11-14 原装 Mac 客户端的两次隔离验证通过：托管后台 0.169.0，以及上述入口
启动的客户端自带后台 0.167.0-alpha.4.1。后者客户端 PID 53650、后台及连接对端
PID 53643，退出清理确认通过；原始报告 /private/tmp/csb-shared-desktop-final.json。
日用客户端没有在此次开发中切换。
