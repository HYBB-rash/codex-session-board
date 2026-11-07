# 远程迁移整合与真实只读接入

Lorentz 在独立树以六提交交付全部持久主机数据迁移及 23 个新测试。
协调者 fast-forward 至 b99199b，保留版本步进和所有旧测试标识。
迁移为各 SQLite 数据库的在线快照，不提供跨数据库的全局同一时刻快照。

Cicero 交付 owned-only 过滤夹具。真实接线发现 /rpc 握手与固定 SSH
目标参数需补齐；修订包独立解包后 64 项测试通过，未拥有会话 ID 匿名化。
实际 0.2.31 binary 通过同一 canonical proxy 连接已登记 MacBook，
runtimeVerified、nativeVerified 为 true，来源写保护错误为空。
本次无会话写入，outside_allowlist_forwarded_writes 为 0。
原始证据位于 outputs/remote-parity-integration/preflight-0705/。

这只是只读预检，不签收运行阶段、写入、空会话输入页或完整发布。
系统在 2026-11-08 06:54 仍锁屏；旧生产 0.1.87 未更换。
下一步为完整自动验收、隔离执行、真实专用会话写入与客户端 GUI 验收。
