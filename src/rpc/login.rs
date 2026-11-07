use super::*;

const BOOTSTRAP: &str = r#"if [ -z "${SHELL:-}" ] || [ ! -x "$SHELL" ]; then exit 127; fi
CODEX_REMOTE_PAYLOAD="$1"; export CODEX_REMOTE_PAYLOAD
case "${SHELL##*/}" in
csh|tcsh) exec "$SHELL" -i -c 'set loginsh=1; if ( -r /etc/csh.login ) source /etc/csh.login; if ( -r ~/.login ) source ~/.login; exec /bin/sh -c "$CODEX_REMOTE_PAYLOAD"' ;;
nu) exec "$SHELL" -l -i -c 'exec /bin/sh -c $env.CODEX_REMOTE_PAYLOAD' ;;
*) exec "$SHELL" -l -i -c 'exec /bin/sh -c "$CODEX_REMOTE_PAYLOAD"' ;;
esac"#;

const ENVIRONMENT: &str = r#"CODEX_HOME="${CODEX_HOME:-$HOME/.codex}"; export CODEX_HOME
PATH="${CODEX_INSTALL_DIR:-$HOME/.local/bin}:$PATH"; export PATH
"#;

const PEER: &str = r#"import json,os,sys
home=os.environ["CODEX_HOME"]
peer={"uid":os.getuid(),"home":os.path.realpath(os.environ["HOME"]),"codexHome":os.path.realpath(home),"declaredCodexHome":os.path.abspath(home)}
sys.stdout.write(sys.argv[1]+"\n"+json.dumps(peer,separators=(",",":"))+"\n")
sys.stdout.flush()
"#;

pub(crate) fn quote(value: &str) -> Result<String> {
    ensure!(!value.contains('\0'), "远端命令包含 NUL");
    Ok(format!("'{}'", value.replace('\'', "'\\''")))
}

pub(crate) fn wrap(operation: &str) -> Result<String> {
    Ok(format!(
        "sh -c {} sh {}",
        quote(BOOTSTRAP)?,
        quote(&format!("{ENVIRONMENT}{operation}"))?
    ))
}

pub(crate) fn framed(operation: &str, marker: &str) -> Result<String> {
    ensure!(
        marker.len() == 32 && marker.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "启动标记无效"
    );
    Ok(format!(
        "python3 -B -c {} {} && {operation}",
        quote(PEER)?,
        quote(marker)?
    ))
}

#[cfg(test)]
#[path = "login_tests.rs"]
mod tests;
