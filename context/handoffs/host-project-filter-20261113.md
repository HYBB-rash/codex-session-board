# 主机筛选实现交接

- 代码：分支 `codex/host-project-filter`，工作树 `/Users/hongyong/Developer/codex-session-board-host-filter`，候选 v0.3.57；基线及范围见同名 active 任务卡。
- 完成：先主机后项目，搜索保留主机范围，新建与批量归档固定目标，离线缓存和项目移除不扩大范围；异常先说明影响，再展开技术诊断。
- 验证：49项浏览器交互、48组五/六列布局、6项真实Rust服务连接隔离原生fixture的检查通过。
- 证据：`/Users/hongyong/Developer/codex-session-board-evidence/host-filter-20261113`，含截图、runtime-smoke.json、layout/measurements.json及acceptance-release.json。
- 完整验收：release入口退出2，629项自动检查通过，31条需求中17项通过、14项待真实环境证据，无失败项，release_ready=false；报告指纹与最终原生fixture候选指纹一致。
- 集成：主工作区另有remote-section-latency调查的未提交记录；保留其修改，按主线最新版本规则集成两次提交并合并上下文，再复验。
- 清理：隔离服务与浏览器已确认停止；没有安装、重载或重启日用插件，没有推送。
- 附注：全仓cargo fmt检查存在原有格式差异；本次修改的Rust测试文件单独检查通过，未改无关格式。
