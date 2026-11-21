# 本地合并与发布

Herman 点名一个分支后，由该分支作者在独立工作树完成合并。远端 `main` 只接受快进，禁止强推和删除。合入结果以远端读回为准。

## 作者合入步骤

1. 在自己的工作树运行 `git fetch origin main` 和 `git rebase origin/main`。处理冲突并审阅 `origin/main..HEAD` 的最终改动。
2. 对最终提交运行 `./scripts/check` 及与改动相关的测试，记录基线、提交号、执行命令和结果。需要复查源码包时运行 `./scripts/validate --scope preflight --output /tmp/board-source-check`。
3. 在干净的本地 `main` 检出获取最新远端分支。先快进至 `origin/main`，确认它仍是验证时的基线，再用 `git merge --ff-only` 合入验证过的提交并普通推送。
4. 读回 `refs/heads/main`，确认目标提交已进入主线。基线在推送前已变，或推送因远端前进被拒绝时，回到作者分支重新 rebase、验证。下一条分支由 Herman 安排。

第三步使用下面的命令。`VALIDATED_BASE` 和 `VALIDATED_COMMIT` 是第二步记录的完整提交号。日用 `main` 若有未提交、未推送或冲突中的改动，使用单独的干净检出，保留原工作树供原任务处理。

```bash
set -e
git fetch origin main
git switch main
test -z "$(git status --porcelain)"
git merge --ff-only origin/main
test "$(git rev-parse HEAD)" = "$VALIDATED_BASE"
test "$(git rev-parse origin/main)" = "$VALIDATED_BASE"
git merge --ff-only "$VALIDATED_COMMIT"
test "$(git ls-remote origin refs/heads/main | cut -f1)" = "$VALIDATED_BASE"
git push origin main
git ls-remote origin refs/heads/main
```

普通开发提交保持现有版本，并通过 `./scripts/check --pre-commit`。每次至多 13 个文件、400 行。日常合入无需 PR 或远端 CI。

## 发布

Herman 安排发布时，在拟发布源码上执行 `./scripts/check --prepare-release`，提交版本变更，再按同样的本地流程合入。主线更新触发 `plugin-release.yml`，定时调和用于补偿触发遗漏。流水线取得执行权后固定最新的主线升版提交，已发布的版本直接复用。运行中的发布继续完成。

Linux 和 macOS 都从固定 SHA 运行完整自动验证。汇总作业以同一运行中的平台作业和报告为准，核对提交、树、指纹、运行平台、日志和清理凭据。通过后，一个作业准备按源码绑定的 Release ID。两个打包作业向该 ID 上传包、校验和、隔离启动和故障场景证据，最终发布再按同一个 ID 读回。发布不可变 Release 后，一次原子推送更新两个候选渠道。

```bash
./scripts/release main status --source MAIN_SHA
./scripts/release main dispatch
gh workflow run plugin-release.yml --ref main -f verify_source=MAIN_SHA
```

`verify_source` 接受已在 `main` 历史中的完整 SHA，保存双平台验证和汇总证据，渠道保持原状。发布失败保留上一次可用的候选渠道，并在 GitHub 工单登记实际源码、阶段和失败运行。修复分支提交写明 `Release-Fix: #工单编号`，双平台候选读回同一发布源码后确认恢复。

完整 31 条验收和稳定渠道继续要求目标环境证据。安装、客户端加载和现场验收分别取证。

## 安装后的验证

安装后的独立生效检查运行
`./scripts/verify-plugin --client-pid CLIENT_APP_SERVER_PID --expected-source RELEASE_SOURCE_SHA --config-id EXPECTED_CONFIG_ID --output /tmp/plugin-proof.json`。
默认只读，核对安装与缓存全部文件、实际HTTP服务身份、配置和数据目录、
MCP可执行文件、所选客户端的父子进程关系及检查期间进程身份未变化。
不根据旧安装回执或无关守护进程宣称当前客户端加载成功。
退出0表示已核实，退出2表示已安装但仍待当前客户端加载，退出1表示核验失败。
可选`--reconnect`复用更新工具已有的定向重连；没有受支持的当前桌面连接时仍返回待重连。
入口优先使用已有交付Python环境，缺少重连依赖时记录失败原因，不输出未核实的成功。
不能启动另一个看板进程冒充成功。报告保存配置摘要，未提供`--config-id`时不代签配置内容已加载。

现场冒烟运行
`./scripts/smoke-plugin --receipt /tmp/plugin-proof.json --session-id EXPLICIT_TEST_SESSION_UUID --output /tmp/plugin-smoke`。
它再次调用生效检查，对实际服务页面执行筛选、取消、空白原因、备注清空和真实等待反馈。
仅操作明确指定的本机卡片，逐次核对服务身份与版本，结束后用服务原有版本保护恢复人工状态和备注。
发现并发修改时拒绝覆盖，报告恢复错误；不会回滚数据库、版本或历史记录。
每次写入前保存恢复记录，保留截图和原始错误，核验全部原备注及配置，清理本次独立浏览器。
默认要求关闭标题、分区同步及自动归档；其他模式应在隔离环境安排专门验收。
此入口记录的是实际HTTP页面，不冒充原生插件容器内的指针拖动或完整31条验收。
每次使用新的源码外报告目录。三个CLI均记录实际耗时，原失败材料不覆盖。
