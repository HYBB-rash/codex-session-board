# 补齐形式化业务覆盖并优化重放

owner: codex
execution_mode: serial_write
allowed_write_set: src/formal/, acceptance/bindings.json, Docs/, Cargo.toml, Cargo.lock, plugin.json, .codex-plugin/plugin.json, context/
base_commit: be7d079e2a287a375223852b54bec4950366bc11
worktree_path: /home/herman/文档/notion-program
coordinator: codex

用户授权先补齐业务验证，再优化数据库重放与有等价依据的剪枝。
主代理串行写入；两个只读代理分别审查状态抽象和连续版本/重启检查。
不修改冻结需求、参考模型与锁，不部署、不推送、不改生产数据。

## 验证目标

- 输入先于分区读回、同轮 pending 重试、真实持久化重启均有连续路径检查。
- 状态及输入版本冲突检查返回值、增量与拒绝后的数据保留。
- 草稿检查首次/重复创建返回值、请求身份与明确拒绝。
- 保留完整抽象边的参考重放，验证优化前后结果和业务断言一致。
- 比较相同构建条件的运行时间；运行完整验收，缺少外部证据如实记录。

## 基线

当前 1,077 状态、37 事件、39,849 条原生转换及 30 条草稿转换。
三项 formal 测试两次耗时 7.94 秒和 7.89 秒，不含编译。
只读审查已复现输入历史和 pending 同轮关系丢失导致模型与 Store 分歧。

## 进度

草稿 30 条边已检查首次/重复创建标志、返回记录、请求/项目/会话身份、
待核对索引及明确拒绝后的记录保留；定向测试通过。
随后修正原生模型和连续业务检查。
