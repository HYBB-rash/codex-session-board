# Codex Enhancer POC

这个目录验证一件事：在不改 `/opt/codex-desktop` 的前提下，让官方 Codex Desktop 的测试副本加载仓库外面的 Runtime，并在官方窗口的导航里打开自己的页面。

当前机器上的目标是 Fedora 44 的 `codex-desktop`，版本 `26.1111.71704`，运行时类型是 Owl。安装器会自己读取本机位置、版本和 `app.asar` 里的 `package.json` `main`，不把入口文件名写死。

## 使用

安装、查看和卸载：

```bash
./scripts/install.sh
node bin/codex-enhancer.js status
./scripts/launch.sh
./scripts/uninstall.sh
```

`install` 用 reflink 复制 `/opt/codex-desktop` 到 `state/test-app`，备份原始 `app.asar`，再给启动入口加上 Loader。Loader 只在测试副本里，通过 `.codex-enhancer-link.json` 指向本仓库的 `runtime/main.js`。

`launch` 使用单独的应用 ID `codex-enh-poc`、`state/user-data` 和 `state/codex-home`。它不会把 `--user-data-dir` 指到 `~/.config/Codex`。窗口关掉之后，再执行卸载。安装和卸载发现测试副本仍在运行时会停止并报出进程号。

登录后的官方导航出现时，Session Board 入口挂在原有侧边栏里。点击后页面画在当前窗口的内容区，Back 会把它去掉。当前测试配置停在登录页，这一页没有导航轨，入口不会提前画出来。调查和这次运行的证据在 `docs/ui-integration-report.md`。

验证时可以在入口出现后自动打开页面：

```bash
CODEX_ENHANCER_AUTO_OPEN=1 ./scripts/launch.sh
```

页面应显示：

```text
Session Board
Codex Enhancer UI integration successful.
```

`status` 在这些情况下报告 `patch-stale`：测试副本的归档哈希变了，Loader 标记不在了，副本里的版本或入口变了。它同时比较系统安装版本和安装时记录的版本。把更新后的副本重新执行 `install`，会备份新的归档并重新打上 Loader，不会用旧备份覆盖新文件。

## 布局

源码是 TypeScript，在 `src/`。`npm run build` 把它编译成 CommonJS。`scripts/` 里的脚本会先编译，再执行 `bin/codex-enhancer.js`。Electron 加载编译结果 `runtime/main.js`、`runtime/preload.js` 和 `extensions/*/extension.js`。沙箱里的 Preload 不能加载旁边的文件，所以 `preload.js` 在构建时已经包含宿主。

```text
src/bin/codex-enhancer.ts
src/installer/      detect, asar patch, install, restore, status
src/runtime/        main.ts, preload.ts, host.ts, registry.ts
src/extensions/     hello-world, session-board
scripts/            build.sh, install.sh, launch.sh, uninstall.sh
state/              test copy, backup, install.json; not part of source
logs/               runtime log and captured window images
docs/verification-report.md
```

`npm test` 先编译，再检查归档读写，不启动 Codex。

## 边界

安装器拒绝修改它探测到的系统安装目录。测试只改 `state/test-app`。

这次没有关闭 Electron fuse，也没有关闭 Chromium sandbox。`EnableEmbeddedAsarIntegrityValidation` 在这个 Linux 二进制上是打开的，但修改归档后程序仍然启动。报告里把这件事和尚未验证的部分分开写。
