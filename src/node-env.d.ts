declare namespace NodeJS {
    interface ProcessEnv {
        CODEX_ENHANCER_AUTO_OPEN?: string;
        CODEX_ENHANCER_LOG_STDOUT?: string;
    }

    interface ProcessVersions {
        electron?: string;
    }
}
