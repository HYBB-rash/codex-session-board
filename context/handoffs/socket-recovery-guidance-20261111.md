# 连接恢复提示待发布验收

owner: codex
task: context/tasks/active/socket-recovery-guidance-20261111.md
worktree_path: /home/herman/文档/notion-program
base_commit: 2bafb7b

代码版本0.2.79已实现易懂连接异常、完整退出重开的指导、“连接帮助”及“重新检查连接”。
重启接口核对发现客户端内部IPC受信任校验，插件协议没有公开生命周期方法。
Linux托盘正常退出有可研究的入口，但未执行，不能据此宣称可安全自动重启。
退出会停止本地authority，真实验收须等运行任务结束后进行，不能杀当前桌面代替验证。

四项专项连接测试、一项标题同步集成测试及浏览器正反场景通过。
浏览器覆盖恢复指导可见、部分失败不冒充全部暂停、远程多错误展示、重新检查失败、窄屏布局及原卡片保留。
完整验收及提交钩子结果保存在 outputs/socket-recovery-guidance-20261111/。
完整验收退出2，499/499自动测试通过，14条外部验收待补，release_ready=false。
实现指纹5fdb9a8197a51434f467d652dc4e1284ee85e4c7bdbd61140bf23986439bb31d；正式仍为0.2.73、PID3296170。
接手时先核对报告的automated_passed、release_ready、源码指纹和当前正式版本。
真实重启恢复及其余31条完整外部证据未补齐前，不宣称发布就绪，不替换正式插件。
本轮未改客户端安装包、正式配置或用户会话，未执行客户端退出、重启或真实移列。
