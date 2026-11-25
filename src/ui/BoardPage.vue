<script setup lang="ts">
import { onBeforeUnmount, onMounted, useTemplateRef } from "vue";
import type { BoardInstall } from "./board-pwa";
import { type BoardRequest, type BoardSnapshot, startBoardPage } from "./board-start";
import moveDragon from "./move-dragon.webp?inline";

const props = defineProps<{
    request: BoardRequest;
    replaceAccount(snapshot: BoardSnapshot): void;
    snapshot?: BoardSnapshot;
    close?: () => void;
    install?: BoardInstall;
}>();
const page = useTemplateRef("page");
let unmount: (() => void) | undefined;

onMounted(() => {
    if (!page.value) throw new Error("缺少看板页面");
    unmount = startBoardPage(
        page.value,
        props.request,
        props.replaceAccount,
        props.snapshot,
    ).unmount;
});
onBeforeUnmount(() => unmount?.());
</script>

<template>
    <div class="board-page" ref="page">
        <main class="shell">
            <header class="page-header">
                <h1>会话</h1>
                <div id="health" class="health">
                    <span id="health-summary" role="status">正在读取会话…</span>
                    <button
                        id="health-more"
                        type="button"
                        class="row-action"
                        aria-label="查看连接详情"
                    >
                        详情
                    </button>
                </div>
                <div class="header-actions">
                    <button v-if="close" type="button" class="quiet" @click="close">
                        关闭看板
                    </button>
                    <button id="refresh" class="quiet" type="button">刷新</button>
                    <button
                        id="plugin-update"
                        class="quiet"
                        type="button"
                        hidden
                        aria-controls="plugin-update-status"
                    >
                        更新
                    </button>
                    <span id="archive-results-control" style="display: contents;"></span>
                    <button id="ssh-settings" class="quiet" type="button" hidden>连接设置</button>
                    <button
                        v-if="install"
                        id="install-app"
                        class="quiet"
                        type="button"
                        :hidden="!install.available"
                        @click="install.prompt"
                    >
                        安装应用
                    </button>
                    <button id="new-session" class="primary" type="button">＋ 新建会话</button>
                </div>
            </header>
            <div id="plugin-update-status" class="health" role="status" hidden></div>
            <div id="archive-status" class="health" role="status" hidden></div>
            <div id="archive-results"></div>
            <div class="toolbar">
                <div class="filters" id="board-filters"></div>
            </div>
            <div class="scope-row">
                <nav class="jump" id="jump" aria-label="跳转到状态列"></nav>
                <div id="filter-scope"></div>
            </div>
            <div id="feedback" class="feedback" role="status" aria-live="polite"></div>
            <details class="unplaced" id="unplaced" hidden>
                <summary id="unplaced-summary">未归列会话</summary>
                <div id="unplaced-cards" class="cards"></div>
            </details>
            <div id="board-workspace">
                <section
                    class="board-viewport"
                    id="board-viewport"
                    aria-label="横向滚动查看全部状态列"
                ></section>
            </div>
            <div id="reply-preview"></div>
            <div id="move-slot">
                <aside id="move-feedback" class="move-feedback" aria-label="状态保存进度" hidden>
                    <div class="dragon-scene" aria-hidden="true">
                        <img id="move-dragon" :src="moveDragon" width="768" height="512" alt="">
                    </div>
                    <div class="move-copy">
                        <strong id="move-heading"></strong>
                        <p id="move-detail"></p>
                    </div>
                    <button
                        id="move-dismiss"
                        type="button"
                        class="quiet"
                        aria-label="收起提示，保存会继续"
                    >
                        收起
                    </button>
                </aside>
            </div>
            <details class="board-help">
                <summary>使用说明与连接帮助</summary>
                <p class="foot" id="foot">
                    拖动卡片修改状态。“待归档”列的一键归档处理当前筛选下本机和远程主机中的可归档会话，也可以逐张归档。只读记录不会归档。新建会话只打开输入页，不发送输入。
                </p>
                <button id="connection-help" class="quiet" type="button">连接帮助</button>
            </details>
        </main>
        <dialog id="health-details" aria-labelledby="health-details-title">
            <h2 id="health-details-title">连接与会话读取</h2>
            <p class="reason">
                先查看受影响的功能，再选择处理办法。选择主机只改变显示范围。后台定时归档仍按原配置运行。
            </p>
            <div id="health-detail-list"></div>
            <div class="dialog-actions">
                <button id="health-close" type="button">关闭</button
                ><button id="health-help" type="button">连接帮助</button
                ><button id="health-recheck" type="button">重新检查</button>
            </div>
        </dialog>
        <div id="card-editor"></div>
        <dialog id="project-assignment" aria-labelledby="assignment-title">
            <form id="assignment-form">
                <h2 id="assignment-title">确认新会话的项目归属</h2>
                <p id="assignment-thread" class="dialog-title"></p>
                <label for="assignment-project">归入项目</label
                ><select id="assignment-project" required></select>
                <p class="reason">
                    请核对这是你刚发送第一句话的会话。这里只登记项目，不发送消息；失败后仍处理同一会话。
                </p>
                <div id="assignment-error" class="notice" role="alert"></div>
                <div class="dialog-actions">
                    <button type="button" id="assignment-cancel">取消</button
                    ><button type="submit" id="assignment-save" class="primary">
                        确认归入项目
                    </button>
                </div>
            </form>
        </dialog>
        <dialog id="new-target" aria-labelledby="new-target-title">
            <form id="new-target-form">
                <h2 id="new-target-title">选择新会话的位置</h2>
                <label for="new-host">主机</label><select id="new-host" required></select>
                <label for="new-project">在这个项目中打开新会话</label
                ><select id="new-project" required></select>
                <p id="new-target-hint" class="reason">使用客户端中已登记的项目。不会发送输入。</p>
                <div class="dialog-actions">
                    <button type="button" id="new-target-cancel">取消</button
                    ><button id="new-target-open" type="submit" class="primary">打开新会话</button>
                </div>
            </form>
        </dialog>
        <dialog id="native-consent" aria-labelledby="native-consent-title">
            <form id="native-consent-form">
                <h2 id="native-consent-title">确认连接和项目改动</h2>
                <p id="native-consent-target"></p>
                <ul id="native-consent-changes"></ul>
                <div class="dialog-actions">
                    <button id="native-consent-cancel" type="button">取消</button>
                    <button class="primary" type="submit">同意并打开</button>
                </div>
            </form>
        </dialog>
        <dialog id="ssh-setup" aria-labelledby="ssh-setup-title">
            <form id="ssh-setup-form">
                <h2 id="ssh-setup-title">远程新建连接设置</h2>
                <fieldset id="ssh-setup-fields">
                    <label>SSH 主机<input name="hostname" required maxlength="253"></label>
                    <label>SSH 用户<input name="user" required maxlength="128"></label>
                    <label
                        >SSH 端口<input
                            name="port"
                            type="number"
                            value="22"
                            min="1"
                            max="65535"
                            required
                        ></label
                    >
                    <label
                        >远程项目绝对目录<input
                            name="directory"
                            required
                            maxlength="4096"
                            placeholder="/path/to/project"
                        ></label
                    >
                </fieldset>
                <section id="ssh-setup-preview" aria-label="配置改动预览" hidden>
                    <p id="ssh-setup-target"></p>
                    <ul id="ssh-setup-changes"></ul>
                </section>
                <label id="ssh-setup-alias-choice" hidden
                    >请选择要复用的 SSH 别名
                    <select id="ssh-setup-alias"></select>
                </label>
                <div id="ssh-setup-saved" hidden></div>
                <p id="ssh-setup-notice" class="reason" role="status"></p>
                <div class="dialog-actions">
                    <button id="ssh-setup-close" type="button">关闭</button>
                    <button id="ssh-setup-decline" type="button" hidden>拒绝</button>
                    <button id="ssh-setup-restore" type="button" hidden>重新启用</button>
                    <button id="ssh-setup-recheck" type="button" hidden>重新检查</button>
                    <button id="ssh-setup-next" class="primary" type="submit">检查并预览</button>
                </div>
            </form>
        </dialog>
        <dialog id="draft-recovery" aria-labelledby="draft-recovery-title">
            <h2 id="draft-recovery-title">核对新建结果</h2>
            <p id="draft-message" class="reason"></p>
            <p class="reason">
                请先在客户端检查是否已经出现新会话。重试会继续处理同一次新建；重新新建会保留此前可能已经创建的会话。
            </p>
            <div class="dialog-actions">
                <button type="button" id="draft-later">稍后处理</button
                ><button type="button" id="draft-retry">重试打开</button
                ><button type="button" id="draft-restart">已核对，重新新建</button>
            </div>
        </dialog>
        <dialog id="connection-recovery" aria-labelledby="connection-recovery-title">
            <h2 id="connection-recovery-title">恢复 Codex 连接</h2>
            <p>连接不可用时，执行状态或侧栏位置可能暂时无法更新。请先保留未保存的输入和备注。</p>
            <ol>
                <li>等待正在运行的任务结束。退出 Codex 可能中断任务。</li>
                <li>
                    从 Codex 菜单或系统托盘选择“退出”，然后重新打开
                    Codex。仅关闭窗口可能没有退出客户端。
                </li>
                <li>重新打开看板，点击“重新检查连接”，并查看上方连接状态是否恢复。</li>
            </ol>
            <p>
                如果仍未恢复，请核对客户端是否启动、插件连接配置是否正确，再向维护者提供错误信息。
            </p>
            <p class="reason">
                远程主机连接异常时，请检查对应主机和连接；重启本机 Codex 不保证能恢复远程连接。
            </p>
            <div class="dialog-actions">
                <button id="connection-close" type="button">关闭</button
                ><button id="connection-recheck" class="primary" type="button">重新检查连接</button>
            </div>
        </dialog>
    </div>
</template>
