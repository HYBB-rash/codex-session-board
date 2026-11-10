# 后台启动不再放在库里

`service::launch` 已从库中移除。向 `src/mcp.rs` 写入对它的调用后，编译器报 `cannot find function launch in module service`。
`Action::Mcp` 改成同时调用 `managed_launch::launch` 时，`build.rs` 报插件入口只能调用 `mcp::serve`。
在 `src/mcp.rs` 里写 `.arg("serve")` 时，编译同样失败。
`AGENTS.md` 不再记载这条限制。
