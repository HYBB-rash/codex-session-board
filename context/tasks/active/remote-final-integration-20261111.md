# 远端恢复、安装入口与最终整合验收

owner: codex
execution_mode: coordinated_isolated_workers
allowed_write_set: src/, tests/, assets/, index.html, scripts/, acceptance/bindings.json, acceptance/test_remote_release.py, Docs/, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/, outputs/
base_commit: 2f63a5a33616e92a9b0a69d99725b253c0600234
worktree_path: /home/herman/.codex/worktrees/remote-final-integration/notion-program
coordinator: codex

沿用本会话授权：完成远端取消归档恢复、PWA安装入口、当前源码完整验收；
仅操作专用验收会话，全部验收通过后直接部署，不再询问部署许可。
取消的新建项目任务保持取消。Hermes保持关闭，保留生产数据及现有归档偏好。

固定基底含Main稳定布局、五状态绑定/迁移/预检、独立形式模型、build.rs和mjs交付修正。
移植4f2aa5f独有的恢复、持续告警、observer防护、PWA、host.error及远端门禁。
保留两边全部测试和逐需求接线，静态526项为下限，新增缺陷回归相应增加。
修复预检附件缺SHA及不完整引用可能绕过报告防覆盖的问题，先在私有夹具复现再验证。
只增加完整验证所需的外层预算，保留每条内部命令600秒限制及全部断言。

主协调者负责恢复、PWA、整合和最终版本；门禁工作者使用独立worktree及branch，
仅改其被分配的门禁、测试、绑定和唯一结果记录，不操作客户端、会话或生产。
各独立分支提交遵守13文件/400行及版本规则；协调者合并后重新固定最终身份。
冻结31条需求、参考模型和锁不变；同一最终构建通过完整31条及16项、release_ready=true
且证据经内容审阅后才部署。部分检查、旧候选回执和正确URI交付均不替代真实操作。

旧材料保留于remote-restore-acceptance工作树的outputs/remote-restore-20261110；
83旧插件缓存已被另一验收版本替换，原构建包保留同SHA二进制，续接索引已注明。
不调用或覆盖当前由分区任务使用的旧验收MCP别名；最终入口使用本会话独立命名空间。
两本机专用卡已由分区任务于09:21明确释放；释放回执和当场基线均已保存。
当前仅L实例持有本机A/B写集，两卡已取消归档用于验收；N不得同时启用本机写入。
92已冻结f9c68da，完整529项自动检查、23项浏览器组件及19项私有CLI运行检查通过；
这些结果不替代原生MCP界面、系统PWA、N/R真实轮次及完整31/16门禁。
独立L插件已通过官方CLI安装，生产82及S缓存未改；当前工具检索未返回新L。
浏览器拒绝codex协议插件入口，原生输入接口超时；不绕过安全规则、不全局reload。
M1已做一次正常daemon start；后续只读握手确认同一服务和正确Peer，未重启远端服务。
桌面工具仍未列出H项目，但当前global-state注册与实际远端目录均已核对。
远端A/B的旧83归档基线保留，新R须先取得当场基线再写；唯一新空会话仅限既有M1项目。
本候选N/R真实输入批次尚未激活；旧真实额度不重置，取消的新建项目任务不重启。
