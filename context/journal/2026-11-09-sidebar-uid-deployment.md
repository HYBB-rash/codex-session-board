# 连接目录兼容修复正式上线

用户明确要求替换上线。已构建并部署 v0.2.40，源码 26fab5c，
binary SHA256 为 025818455385153fd8077e84e6097040f9ac44d4bda685551778dfc950df75a9。
正式 marketplace source、插件包、缓存与配置切至 40，运行数据目录保留，
没有迁移数据库。旧正式服务退出，旧启动入口仅指向新版本。
插件安装曾替换预置缓存 inode，造成初次 40 实例路径已删除；
核验 pidfd、启动时间、实际 binary 与 HTTP 身份后重新启动，
最终服务及 4 个 MCP 均来自稳定 cache40 路径且字节一致。
已执行 plugin/install、skills/list forceReload、config/mcpServer/reload。
实际 installed/enabled、MCP version40/toolsError=null、Native 资源读回通过；
原生桥接资源与新 binary 直接返回的资源正文逐字节一致。

release 二进制隔离实测：旧目录不存在时从同级当前 UID 目录完成
只读 list_projects，MCP、HTTP 身份及数据隔离检查通过，未产生会话，
验证实例已停止并清理。源码原 487 项自动检查通过。
5 库完整性与原有 146/100/0/0/44 条备注字段全部核对通过；
配置字段、定时归档策略不变，本会话进行中且原生移列错误清除。
原有 4 个历史位置缺项仍保留，没有扩大数据修复范围。
证据和私有备份位于 outputs/sidebar-uid-deployment-20261109/。
完整 31 条外部签收未重做，不把本次部署记成全量发布验收通过。
收尾 v0.2.41 仅登记上下文，实际部署及验收对象固定为上述 40 产物。
收尾提交首次钩子因 /tmp 的 usrquota 耗尽导致夹具编译失败；
改用项目 target/check-tmp 并限制测试并发重跑，保留全部检查与断言。
