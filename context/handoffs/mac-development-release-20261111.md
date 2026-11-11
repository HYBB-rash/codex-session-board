# Mac 开发迁移与原生插件交付接力

owner: codex-mac-migration
date: 2026-11-12
status: verification_in_progress

## 入口

- Mac 开发目录：`/Users/hongyong/Developer/codex-session-board`。
- Mac 迁移保全：`/Users/hongyong/Developer/codex-session-board-migration/20261111`。
- Fedora 实现检出：`/home/herman/文档/codex-session-board-migration-20261111`。
- 私有远端：`HYBB-rash/codex-session-board`。
- Mac origin 拉取使用 `https://github.com/HYBB-rash/codex-session-board.git`；
  pushurl 保留 `ssh://git@ssh.github.com:443/HYBB-rash/codex-session-board.git`。
- 日常过程与完整命令：`Docs/MAC-DEVELOPMENT.md`、`Docs/DEPLOYMENT.md`。

## 证据与判定

源码上下文记录的是提交时可确认的状态。最终执行结果另存运行回执：

- Fedora `outputs/final-delivery.json`。
- Mac 迁移目录 `final-delivery.json`。
- GitHub CI 的源码 SHA、双平台工作流状态及产物 revision 必须匹配本次 HEAD。
- Mac `install-github-baseline-retry.json` 已确认 GitHub 0.2.98 真实取包及 MCP 重载。
- Fedora `outputs/rollback-pin-probe.json` 已确认原生 CLI 支持固定 Git SHA 回退。
- 后续新版本、回退和再升级回执须逐一检查 phase、实际二进制和 MCP 版本，
  并比较升级前后配置、备注及 SQLite 完整性。保留失败日志。

完整业务验收与交付机制验证分别记录。31 条需求、参考模型及锁保持不变。
稳定发布必须完整验收退出 0、release_ready=true 且证据经审阅；
当前仍有真实环境证据缺口，未把自动范围通过写成完整签收。

## 保全

初次完整 history.bundle 保存所有引用、工作区 HEAD、stash；
各工作区另有 staged/unstaged 补丁、文件归档和 SHA 清单。
133 份开发聊天为只读历史归档；清理任务的额外源树与材料也已复制核验。
最终开发快照已有六工作区、1843 个文件和 137 份聊天；新增检查点保全 140 份聊天，
包含全部 137 份基线，逐文件和归档 SHA 均在 Mac 核对。运行中的 JSONL 只截取完整行。
最终同步须补存统一 main 4641088 及原检出后续引用，
Mac 原导入检出的未提交状态先保存在 stash 和迁移目录再快进 main。
不得用旧快照覆盖最终源码，也不得丢弃其他任务尚未合入的工作。

生产配置、数据库、socket 和全局用户设置未复制为插件内容。
Mac 配置、数据、更新 venv 独立于源码及插件缓存；
现有正式 Fedora 0.2.92 保持原部署，旧 0.2.38 release 内仍有生产 data_dir，
不能清理旧 release。测试使用独立 Linux CODEX_HOME 和 Mac 新配置。

## 签名和认证

Mac `~/.local/bin/agent-git` 使用独立 Agent 身份与系统 ssh-keygen，
普通身份和全局凭据设置保持原样。GitHub Verified 公钥登记未执行。
曾有 SSH agent 拒绝与旧 WebSocket 重载失败，日志保留；
认证恢复后 Mac GitHub 基线安装和实际 MCP 重载成功。
新取包不得把缓存恢复记录当作 GitHub 下载成功证明。
Mac 的既有 op-member 委托登录可用，仓库专用 HTTPS helper 已真实拉取 main。
外部 gitconfig、凭据引用和 helper 位于本机配置数据目录，不含 token 明文。
现存 token 写入及默认分支设置返回 403；不扩大权限、不复制 Fedora 凭据。
源码推送仍由已登记 SSH 身份负责；2026-11-12 的实际 push dry-run 返回 0。
同次尝试先记录一把 agent 密钥拒签，再成功完成，不能据此推断所有密钥均已解锁。
真实 Agent 签名提交和写入远端的 SHA 由最终回执给出。

CI 初跑发现系统 Bash 3.2 空参数数组、暂存缓存嵌入旧目录、
Python 临时目录未规范化以及 Linux 测试磁盘耗时。
修复保持断言和冻结契约；共享 deadline 测试保留 210ms 预算、
600ms 上界，并精确验证初始化、读取不重置绝对 deadline。
失败工作流、日志与报告继续保留。
共享 deadline 回归使用公开 connect、服务端初始化时刻和精确绝对时间，
同时核对读取不刷新预算、过期写入不发送第三条请求，保留原时间上界。
CI 钩子测试在内存临时盘仍触及原 600 秒限额，编译缓存试验记录零命中后移除。
Mac 日志显示前 17 项通过、后续仍在编译；19 项钩子集成测试的进程预算改为 1200 秒，
其余进程保持 600 秒，产品时间断言保持原值。0.3.20 双平台 CI 完整成功。
Fedora 和 Mac 独立解包源码的完整验收均为 558 项自动通过、17 需求通过、14 待证，
实现指纹同为 2677d9b9dd06176db1f102183de4a7b3ceeb6ed92b619deea63c6b3768b8361e。
Mac 升级曾在全局 skills/list 刷新超时后保留 pending；定向配置重载和目标 MCP 读回
约 1.5 秒成功。本次删除对全局扫描的依赖，仍严格核对真实插件 ID、版本、工具和错误字段。
Mac 恢复试验已成功升级与回退，但冗余 marketplace upgrade 下载两次出现 early EOF。
安装流程改为直接添加已校验的完整平台包 revision，再安装插件；移除紧接添加后的重复刷新，
仍读回原生缓存 revision 和全部发布哈希。分支只用于预取，安装固定本次已验证快照。
0.167.1 的 plugin add 正常返回成功，但缓存没有旧代码依赖的内部标记。
来源校验改用原生市场实际 Git HEAD；安装缓存仍须与预取发布清单逐字段、逐文件一致，
已有安装标记也必须一致。未创建或伪造内部标记。
修订控制器已完成 Mac 0.3.20 → 0.2.98 → 0.3.20，全程配置及备份保留，
三个阶段均读回实际服务和 MCP 版本。回执为 mac-roundtrip320-native-provenance/summary.json；
这是修订工作区控制器的验证，提交后的版本仍须自己的 CI 与新原生包闭环。
Mac 文稿出现旧文件“2”后缀副本，源码指纹保护实际拦截；
旧目录及全部副本先保全，再从 GitHub 在 Developer 新克隆并导入历史引用。
