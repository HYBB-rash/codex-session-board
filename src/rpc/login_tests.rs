use super::*;

#[test]
fn shell_quote_preserves_metacharacters_quotes_newlines_and_unicode() -> Result<()> {
    for value in [
        "literal $HOME `id` $(id); | &",
        "a'b\"c\\d\n第二行",
        "",
        "--argument",
    ] {
        let output = Command::new("sh")
            .arg("-c")
            .arg(format!("printf %s {}", quote(value)?))
            .output()?;
        assert!(output.status.success());
        assert_eq!(output.stdout, value.as_bytes());
    }
    assert!(quote("bad\0value").is_err());
    Ok(())
}

#[test]
fn desktop_login_wrapper_shares_home_install_dir_and_literal_payload_for_proxy_and_clock()
-> Result<()> {
    let directory = tempfile::tempdir()?;
    let shell = directory.path().join("login-shell");
    std::fs::write(
        &shell,
        "#!/bin/sh\n[ \"$1\" = -l ] && [ \"$2\" = -i ] && [ \"$3\" = -c ] || exit 9\nexport CODEX_HOME='/profile/codex home'\nexport CODEX_INSTALL_DIR='/profile/codex bin'\nexec /bin/sh -c \"$4\"\n",
    )?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o700))?;
    for phase in ["proxy", "clock"] {
        let operation = format!(
            r#"printf '%s\n%s\n%s\n%s\n' {} "$CODEX_HOME" "$CODEX_INSTALL_DIR" "$PATH""#,
            quote(phase)?
        );
        let output = Command::new("sh")
            .arg("-c")
            .arg(wrap(&operation)?)
            .env("SHELL", &shell)
            .env("HOME", directory.path())
            .env("PATH", "/usr/bin:/bin")
            .output()?;
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout)?,
            format!(
                "{phase}\n/profile/codex home\n/profile/codex bin\n/profile/codex bin:/usr/bin:/bin\n"
            )
        );
    }
    Ok(())
}

#[test]
fn wrapper_fails_closed_without_executable_login_shell_and_keeps_nonce_out_of_plain_command()
-> Result<()> {
    assert!(
        !Command::new("sh")
            .arg("-c")
            .arg(wrap("exit 0")?)
            .env("SHELL", "")
            .status()?
            .success()
    );
    assert!(
        !Command::new("sh")
            .arg("-c")
            .arg(wrap("exit 0")?)
            .env("SHELL", "/nonexistent")
            .status()?
            .success()
    );
    assert!(framed(":", "wrong-marker").is_err());
    assert_ne!(framed(":", &"a".repeat(32))?, framed(":", &"b".repeat(32))?);
    Ok(())
}
