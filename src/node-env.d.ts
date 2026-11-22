declare namespace NodeJS {
    interface ProcessEnv {
        CODEX_CLI_PATH?: string;
        CODEX_ENHANCER_AUTO_OPEN?: string;
        CODEX_ENHANCER_BRIDGE_SOCKET?: string;
        CODEX_ENHANCER_LOG_STDOUT?: string;
        CODEX_ENHANCER_SOURCE?: string;
        CODEX_ENHANCER_STATE_DIR?: string;
        CODEX_ELECTRON_USER_DATA_PATH?: string;
        CODEX_LINUX_APP_ID?: string;
        CODEX_LINUX_APP_SERVER_BRIDGE_SOCKET?: string;
        CODEX_SPARKLE_ENABLED?: string;
        XDG_RUNTIME_DIR?: string;
    }

    interface ProcessVersions {
        electron?: string;
    }
}
