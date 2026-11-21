# 双入口首件

Linux 插件包来自 `b794c8a076760e7d5ea6936a118a168b57f7fa95`，包内二进制 SHA256 为 `5865ca6050a1f3925ac1ba1024e48049826dd31aca5b37364669cee2d834b910`。标准打包器生成了含 `SOURCE.tar.gz` 的插件目录，`scripts/package_scenarios.py` 将其打包、重新解包并核验清单。五项隔离启动与故障检查通过。

## 两个入口的操作结果

网页直接打开解包进程提供的 HTTP 首页。嵌入宿主只提供外层框架和工具转发，内页是同一包的 `resources/read(ui://session-board/index.html)` 返回的 1,000,386 字节 HTML，SHA256 为 `edad9e7f0efb2203c8cbc31c7c2ddc77e3f98c1e9a52f9bdef737815e92a70ee`。宿主按原样转发 MCP 请求和结果，包括预览的 `_meta`。

两个入口分别使用同样的三个隔离会话。点击“开发会话 2”看到“还没有完整回复”。第一次保存备注后版本为 2，第二次输入两个空格后独立读回空备注，版本为 3。点击待归档列排序并选“新的在前”后，服务返回 `desc`。搜索“开发会话 2”后点击列头“一键归档”，服务记录的唯一目标是 `11111111-1111-4111-8111-000000000002`，状态版本 3，输入版本 0，原生位置与保存后读回相同。已在待归档列但被搜索排除的 3 号会话仍在原列。关掉页面并重新打开后，从“归档结果”读到了同一批次。

首次嵌入复验采用仅允许脚本的 iframe，浏览器阻止表单提交，后端未收到保存。首件宿主随后开启 `allow-forms`，保留沙箱的独立来源。两个入口在最初的原生 fixture 中都得到真实归档拒绝，包日志指出 `initialize` 缺少 `codexHome`。只给原生 fixture 增加自己的 Codex 目录和无账号应答后，使用原包重新完成上述读回。该 fixture 可执行文件 SHA256 为 `bd756fe089a286a8afab4ec30a36b10df367f8cf2fed2dfac7ce10529045d123`。

首件报告位于源码外工作目录 `frontend-packaged-actions/first-http-3/`、`first-mcp-3/`。两个报告均记录了包归属、进程清理和浏览器 profile 清理，早先失败的原始报告另存。

## 最终主线仍待验证

本轮恢复的是已完成的单目标批次。E3 的十卡中断、未提交和待核对状态、重开后只核对而不重复写入，需在 E3 主线的隔离 fixture 上验证。E1、E2、筛选与排序的最终组件也须用同一包的两个入口重演。客户端宿主授予的沙箱能力、原生指针交互、PWA 实际安装和独立窗口启动另行现场验收。

## T0 · 独立网页安装与离线访问

保留 `test::board_io::real_http_pwa_manifest_and_assets_are_local_and_uncached` 及其现有绑定。受益人是从本机浏览器打开或安装看板的用户。`context/tasks/completed/pwa-support.md` 记载了安装入口和服务不可用时的重新连接页。

保留真实 GET 的清单字段、资源同源性、内容类型、`no-store`、脚本 `nosniff` 和令牌占位符断言。浏览器解析首页后检查安装清单、安装脚本和实际可解码的内嵌图片，取代要求标签属性按某一顺序排列的三个断言。检查非空图标清单，防止没有安装图标时空循环通过。

同一测试先打开有会话的隔离 Rust 服务，再停止该进程。浏览器重访首页，已登记的 Service Worker 返回 503 及本机重新连接入口，离线访问会话 API 失败，Cache Storage 中没有首页和会话接口响应。恢复隔离服务后，点击“重新连接”回到看板。主机与写入校验继续由同文件的 `real_http_pwa_assets_preserve_host_and_mutation_checks` 负责。

## T0 · 官方远程新建入口

原 ID 为 `acceptance.test_native_new.NativeNewTests.test_official_entry_has_no_client_rewriting_tools_or_capability_gate`。
使用原装 Codex 的用户从插件点选登记的 SSH 别名和项目目录，支持的入口是 `board_new` 或 HTTP `/api/new`。`Docs/REMOTE-NATIVE-NEW.md` 规定插件不得修改、复制或重签名客户端。导航仅交给已配置的启动器，历史请求恢复另有测试。

- 源码保护改名为 `acceptance.test_native_new.NativeNewTests.test_official_entry_keeps_retired_client_patch_tools_out_of_source`。保留对八个已撤下改写工具及原生导航模块 `app.asar` 引用的检查。这九个定点检查能发现已知旧实现回归，无法证明任意实现对整个操作系统零写入。
- 官方 URL 与两条退役 URL 的源码字符串检查交给 `app::drafts::tests::native_remote_new_only_navigates_to_the_registered_ssh_alias_and_directory`。隔离启动器记录所有实参，仅接受一个实际导航 URL，逐项核对 `codex` scheme、`settings` 主机、路径、别名、目录、启用与激活参数，并读回登记文件原文。规范远程项目入口由 `app::target_drafts::tests::canonical_target_request_uses_verified_rpc_and_only_opens_native_input` 核对同一协议。
- 能力门槛交给 `config::tests::obsolete_native_capability_does_not_require_a_client_patch_or_existing_file`。它加载指向不存在文件的旧配置，核对读取成功且序列化后忽略旧字段。原绑定同步指向这些承接者。

这些测试采用临时目录、启动器和合成账户资料。观察范围是插件源码、实际导航参数和隔离登记文件；没有运行退役改写器，也未操作日用客户端或真实账户。
