# 插件是单体，不启动后台

## 决定

插件进程就是看板。它跟随 CodeHost（拉起这个插件的 Codex 进程）启动和退出。脱离插件的服务进程不由插件拉起。

会话状态留在数据库里。会话控制的形式化模型不包含进程寿命。

## 理由

看板原先由插件调用 `service::launch`，再把 `serve` 放进新的进程组。CodeHost 关掉或重载插件之后，那个进程还在。Codex 安装器会把插件缓存目录整棵换掉，启动记录里的程序路径就对不上，侧栏拒绝读取。

没有用户需求要求 CodeHost 退出后看板继续扫描或自动归档。本机执行状态来自 Codex 自己的连接。桌面不在时，后台也看不到新的执行。

## 未采纳的做法

把这条限制写进 `AGENTS.md`。那只在读过规则的人遵守时才生效。

只在 `build.rs` 里搜索 `service::launch` 这几个字，同时把 `main.rs` 整份放行。插件入口仍和启动函数在同一个编译单元里，换个写法就能编过。

## 后果

`spawn_service` 和 `launch` 在 `src/managed_launch.rs`。这个文件只由二进制的 `src/main.rs` 引入。库代码调用它时，编译器报错，因为库里没有这个模块。

`codex-session-board launch` 仍从 `main.rs` 的 `Launch` 分支进入。`Action::Mcp` 分支只调用 `mcp::serve`。`build.rs` 在这里对不上就停止编译。库里的其他文件再写 `.arg("serve")`、用当前程序拉起子进程，或在 `rpc/capture.rs` 之外使用 `process_group(0)`，同样停止编译。
