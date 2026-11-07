# 原生与远程专项汇总门禁交付

2026-11-08，Cicero 在独立连接工作树交付，协调者摘录并整合。
原始基线为 10c80203284d3f0fe069d3cb003113cdb26a97f6。
实现已于 61e65e6、6c4cf80 接入；此记录不是产品验收通过声明。

## 入口与覆盖

- `scripts/verify_remote_release.py` 复用冻结执行器的指纹、附件和报告保护。
- `acceptance/test_remote_release.py` 新增 17 项门禁测试。
- 原有 9 项 Python 验收器与分发测试保留，共 26 项通过。
- 原 31 条 requirements、model 与锁不变；完整 release 与专项必须同时通过。
- 全部新增 Python 测试由绑定生成器登记到 GATE-001，不能只跑筛选集合签收。

专项包含原生五状态、外部选择冲突、运行阶段去重、恢复与 UI，
以及远程 canonical active owner、阶段、分页、空会话、丢回执、项目 UI、
标题三种入口与重试、归档保护、主机隔离、导航及真实拖拽。
固定案例的唯一来源为汇总器的 CASES，不在本记录维护第二份编号表。

## 证据约束

manifest 需要 schema_version、observed_source_commit、contract_sha256、
build_sha256、binary、legacy_report、legacy_evidence、modes、checks、
audit、cleanup、review。文件引用均包含 path 与实际 sha256。
每项记录包含 status、operator、environment、带时区 observed_at、notes、
非空 artifacts。未测保持 pending，不把单元测试合成附件当作产品证据。

legacy/native5/remote 指根服务模式，各用不同规范化 data_dir 和 configId，
共同使用同一实际 binary。配置附件为 Rust Config 的完整有效序列化；
info 为实际 /api/info；remote 另带实际配置指向的 registry 镜像及 hostId。
legacy 关闭原生分区、代理、真实归档与 sidebar；其余两者启用原生五状态。
remote 最终登记必须有效；过程中故障、偏好切换及恢复另存原始证据。

原 legacy evidence 除冻结字段外，记录 mode=legacy、configId、
binary_sha256 及观察 commit。完整 legacy report 须由当前源码门禁产生，
scope=release，所有绑定、逐需求及外部项目通过，不能复用旧 binary 报告。

audit 列出本机和远程专用会话、逐条转发写入以及整数 0 的
outside_allowlist_forwarded_writes。cleanup.remaining_owned_resources 必须为空；
保留的证据、测试会话和正式部署资源在 notes 分别说明。
review.subject_sha256 使用 review_digest(manifest)，覆盖除 review 外的全部内容。
审阅者必须实际检查原始日志和截图；机器不判定 GUI、owner 或审计内容真实性。

## 最终版本与提交

先准备最终四个版本文件，暂不提交；构建这份源码并完成所有取证。
观察时记录真实 HEAD，不能在最终提交后改写原 manifest 的观察 commit。
验收成功后只追加 context 归档，并与已经准备的版本一起提交。
重新汇总时允许观察 commit 为当前 HEAD 的祖先，但完整源码指纹、
binary、三种运行 info、legacy 与逐案例指纹仍须完全一致。
报告分别记录 observed_source_commit、final_source_commit、attestation_commit。
任何源码或 binary 变化均需重新取证；复制同字节 binary 到插件包不改变身份。

```bash
python3 scripts/verify_acceptance.py --evidence "$run/legacy-evidence.json" --report "$run/legacy-final.json"
python3 scripts/verify_remote_release.py --manifest "$run/remote-release-manifest.json" --binary "$run/codex-session-board" --report "$run/remote-release-final.json"
```

汇总器只校验已有证据，不执行 binary 或重跑原验收。退出 0 才可签收；
2 表示证据待验证，1 表示结构、身份、安全或已记录失败。
报告不能覆盖源码、manifest、binary、任何直接或间接附件及其链接别名。
