# Linux Chromium singleton 保护设计

本次仅实现插件内保护，不改客户端、timer 或正式配置。Linux 显式配置
`client_singleton_profile`，指向跳转命令对应的本地客户端 profile；默认 null
表示关闭。远程会话仍由本机客户端打开，不能把远端 profile 当成本地配置。
macOS 不启用此 Linux 机制，配置时返回明确错误。

## 判定标准

已有运行实例时，跳转前验证当前 profile、进程、监听 socket 与临时目录的关联。
临时 cookie 完整则不写；仅其缺失可恢复；任何不一致均拒绝启动跳转命令。
冷启动仅允许没有任何 singleton 标记的 profile。残留、启动中的不完整标记
返回可重试错误，不替 Chromium 判断旧实例失效，不删除锁或结束客户端。

## 实例身份与目录持锁

打开配置 profile 和 SingletonSocket 所指目录，使用 DIRECTORY、NOFOLLOW、
CLOEXEC，检查本用户所有且无组/其他用户写权限；临时目录要求 0700。
全部 cookie 操作相对打开的目录描述符进行，避免路径被替换后写到新目录。
共享 BSD flock 非阻塞获取，持续持有至运行时退出或实例换代。

身份包括 profile/临时目录的 dev、ino，profile 三个符号链接的值，
本机 hostname、boot_id、SingletonLock PID、/proc/PID/stat 的启动时钟，
进程 executable 的 dev、ino，以及其当前监听 socket 的内核 inode。
从 /proc/net/unix 查找该路径的唯一 AF_UNIX stream listener，再验证
/proc/PID/fd 包含对应 socket inode。过程不建立连接、不向客户端发消息。
这是同用户运行实例关联校验，不是对已控制同一 UID 的攻击者的安全边界。

进程、profile 和 socket 快照在恢复前后与启动跳转命令前重读并比较。
失去监听、目录 unlink/替换、PID 复用、cookie 换代均使旧快照失效。
不持久化 cookie，不跨重启沿用旧快照。异常时不猜测、不覆盖现有项。

## 条件恢复和并发

后台保护只持锁并报告状态，不后台恢复 cookie；恢复仅发生在用户跳转前。
用 symlinkat 原子创建缺失 cookie；不先删除，不覆盖。EEXIST 时读回并比较，
相同值表示另一个调用完成修复，其他值/类型报错。恢复后再次验证实例。
若并发重启导致后验失败，停止跳转，不尝试删除刚创建的条目，避免删除
新实例或其他修复者已经接管的内容。下一次请求重新获取当前实例快照。

后台每秒重新发现，先取得新目录锁再释放旧锁。瞬时发现失败释放旧快照，
健康状态保留错误；跳转独立重新校验，不依赖后台某次“健康”结果。
跳转局部锁一直保持到辅助启动进程退出；若超出一秒则交给等待线程保管。
运行时关闭停止后台 worker 并释放其锁，辅助启动仍存活时由等待线程持锁。

## 已知边界

flock 保护 systemd-tmpfiles 遍历 /tmp 父目录时的老化清理。部署时应确认
实际清理规则符合此形态。直接以临时目录为清理根的规则、rm/unlink、特权删除不受保证。
缺少 socket 或整个目录不可只靠 cookie 修复，返回人工重启提示。
进程启动/退出和启动命令之间没有 Chromium 提供的跨进程原子事务；前后校验
缩小窗口，目录锁只排除合作清理器，不宣称能排除显式删除或任意并发重启。
后台首次发现前和插件退出后的清理窗口仍存在；下一次跳转可条件恢复。
辅助进程退出 0 只证明命令成功，真实 UI 回到目标会话需独立验收。

## 验证矩阵

- Linux 真实 Unix listener 的 /proc 所属 PID 识别，不产生探测连接。
- cookie 健康、缺失、错误值、普通文件、悬空 symlink 的区分。
- 多个修复者并发，成功后 cookie 相同；错误条目不覆盖。
- 进程退出、PID/启动时间变化、重启 cookie 变化、socket 删除/替换。
- 目录被删除或更名替换，旧描述符不向新目录写入。
- systemd-tmpfiles 隔离父遍历持锁/释放正反例，直接清理根反例。
- 导航失败不执行配置命令；已有/新建/远程打开共用入口。
- 运行时退出释放后台锁；超时启动辅助进程的锁随其结束释放。

研究目录保留 FINDINGS.md/results.json 与本次集成代码的 integration-results.json。
本次重新执行了真实 second-instance 转交、持锁/释放清理和重启换代实验。

上游依据：Chromium process_singleton_posix.cc 的 ConnectSocket/CheckCookie
和 Create；systemd TEMPORARY_DIRECTORIES.md 的 BSD directory lock 约定。

## 当前实现的验证结果

Rust Guard 已接入隔离 Electron 主进程实验，结果保存于研究目录的
`integration-results.json`：缺失 cookie 恢复后第二次启动的 `second-instance`
事件由原 PID 接收；共享目录锁持有时 parent-traversal 的 tmpfiles 老化
保留 cookie，释放后再次老化删除它。实际退出并重启 Electron 后 cookie
换代，旧 Guard 拒绝 prepare，新 Guard 可获取。这些操作只针对独立
profile 和复制的 Electron，未操作正式 Codex profile 或系统 timer。

自动回归覆盖共享锁引用寿命、八路并发修复、标记/目录换代、权限变化、
监听者消失、错误 cookie/普通文件保留、跳转拒绝及 observer 退出释放。
两个运行时重叠时，首个退出不释放另一运行时的保护，最后一个退出才可独占持锁。
跳转辅助进程超过同步等待时间时，由等待线程保留 Guard 至该进程退出。
正式 Codex GUI 的真实会话跳转仍需目标环境验收；隔离 Electron 实验
只证明 singleton 转交机制，不能替代这项验收。

## 接入与验收状态

健康信息的 `singletonProtected` 只表示当前持有目录共享锁；cookie 异常仍会通过错误字段报告，
不能把这个布尔值解释为已经能够跳转。观察器只检查和持锁，补回 cookie 只发生在跳转准备阶段。

完整目标环境验收仍待核对；隔离 Electron 实验不能替代正式 Codex 客户端
的真实会话跳转。正式配置尚未启用，当前工作仅在本地集成。
