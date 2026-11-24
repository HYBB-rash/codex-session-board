# 图形测试环境

`gui-test.py` 接收任意前台测试命令，支持本机 Linux 和通过 SSH 运行的 Mac。命令标准输入输出直接传递，命令返回值由 CLI 返回。`--cwd` 在执行端解析，`--timeout` 默认 900 秒，超时返回 `124`。`GUI_TEST_TMPDIR` 在执行端提供本次专用的临时目录。要保留的截图和日志请写入测试命令自己的产物目录。

## Linux

需要 Python 3.9+、Xvfb、xauth 和 dbus-run-session。每次执行新建 X11 显示、D-Bus 会话和 `XDG_RUNTIME_DIR`。窗口和来自该显示的输入事件留在虚拟屏幕里。测试总线关闭桌面服务自动激活，避免触发通知、钥匙环及门户。需要访问已有连接的测试命令可以继续使用继承的 `SSH_AUTH_SOCK` 和现有测试配置。

```bash
./scripts/gui-test.py -- xdpyinfo
./scripts/gui-test.py --screen 1600x1000 -- node --test acceptance/board-interaction.test.mjs
```

Electron 启动参数可传 `--ozone-platform=x11`。应继续由原有验收脚本选择测试副本、登录配置和专用卡片。需要跨多步保留同一窗口时，把启动、等待、探针和退出放进同一条前台验收命令。若启动器将应用分离到独立进程组，沿用原验收脚本的退出清理。

## Mac

需要现有 SSH 登录、远端 Python 3.9+ 和属于该 SSH 账号的已登录 macOS 图形会话。Mac 窗口使用远端机器的图形会话，可能出现在 Mac 屏幕上。要求 Mac 屏幕也不受干扰时，应选择专用测试机或已有图形登录的 macOS 虚拟机。

```bash
./scripts/gui-test.py --host macbook -- sw_vers
./scripts/gui-test.py --host macbook --cwd '~/Projects/example' \
  --forward 19222:9222 -- ./run-gui-acceptance
```

`--forward 19222:9222` 仅把远端回环端口 `9222` 转到本机回环端口 `19222`，可用于 CDP。多端口可重复指定。SSH 按已有配置验证主机和登录，不启用 X11 或 SSH agent 转发。远端命令使用 SSH 环境。如需 Homebrew 工具，可传其绝对路径。目标文件应位于远端，或由调用方随标准输入传入。现有看板候选上传及清理逻辑可以继续使用原验收脚本。

### 临时 SSH 文件密钥

连接 Mac 的签名不稳定时，用 `--temporary-key` 从一条已授权的 SSH 连接登记一次性公钥。远端只允许登记时的来源地址和本次命令，公钥在命令启动前被撤销，未用的公钥五分钟后失效。测试连接使用 `IdentityAgent=none`，本地私钥随命令退出删除。目标仍按已知主机密钥校验。

已有控制连接时直接传其路径。需要做一批验收时，可先启动一条控制连接，再在本批命令中复用。初次启动沿用现有 SSH 身份。

```bash
ssh_session=$(mktemp -d)
ssh -MNf -S "$ssh_session/control" -o ControlPersist=30m macbook
./scripts/gui-test.py --host macbook --temporary-key --ssh-control "$ssh_session/control" \
  -- /usr/bin/uname -sm
ssh -S "$ssh_session/control" -O exit macbook
rmdir "$ssh_session"
```

单次运行也可以只传 `--temporary-key`，工具会自行建立和关闭初次授权连接。临时密钥适用于直连目标和前台命令。需要 `--forward` 时，使用普通 `--ssh-control`。只需验通 SSH 时，可运行 `python3 -B scripts/ssh_file_key.py`，它接受同样的 `--host`、`--ssh-control`、`--remote-python` 和 `-- COMMAND`。
