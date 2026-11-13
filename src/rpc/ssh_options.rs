use anyhow::{Context, Result, bail};

fn owned(keyword: &str) -> bool {
    matches!(
        keyword
            .split(['=', ' ', '\t'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "controlpath"
            | "controlmaster"
            | "controlpersist"
            | "forkafterauthentication"
            | "clearallforwardings"
            | "forwardagent"
            | "sessiontype"
            | "stdinnull"
            | "remotecommand"
            | "ipqos"
    )
}

fn append_option(
    flag: char,
    suffix: &str,
    arguments: &mut std::slice::Iter<'_, String>,
    retained: &mut String,
    result: &mut Vec<String>,
) -> Result<()> {
    let value = if suffix.is_empty() {
        arguments.next().context("SSH 选项缺少参数")?.as_str()
    } else {
        suffix
    };
    if flag == 'S' || flag == 'o' && owned(value.trim_start()) {
        return Ok(());
    }
    retained.push(flag);
    if suffix.is_empty() {
        result.push(std::mem::replace(retained, String::from("-")));
        result.push(value.to_string());
    } else {
        retained.push_str(value);
    }
    Ok(())
}

pub(super) fn sanitize(arguments: &[String]) -> Result<Vec<String>> {
    let mut result = Vec::new();
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        if !argument.starts_with('-') || argument == "-" {
            result.push(argument.clone());
            continue;
        }
        if argument == "--" {
            bail!("托管 SSH 前缀不能包含目标边界");
        }
        let mut retained = String::from("-");
        for (offset, flag) in argument[1..].char_indices() {
            match flag {
                'N' | 'n' | 'W' | 'O' | 'G' | 'Q' | 'V' | 'Z' | 's' => {
                    bail!("托管 SSH 连接不支持会改变会话模式的 -{flag} 参数")
                }
                'M' | 'f' | 'A' => {}
                'B' | 'b' | 'c' | 'D' | 'E' | 'e' | 'F' | 'I' | 'i' | 'J' | 'L' | 'l' | 'm'
                | 'o' | 'P' | 'p' | 'R' | 'S' | 'w' => {
                    append_option(
                        flag,
                        &argument[offset + 2..],
                        &mut arguments,
                        &mut retained,
                        &mut result,
                    )?;
                    break;
                }
                flag => retained.push(flag),
            }
        }
        if retained.len() > 1 {
            result.push(retained);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn wrapper_arguments() -> Vec<String> {
        [
            "-B",
            "fixture.py",
            "state.json",
            "-i",
            "-A",
            "-Jjump",
            "-o",
            "ProxyCommand=ssh -W %h:%p jump",
            "-voControlPath=none",
            "-S",
            "/tmp/other",
            "-MMfA",
            "-Fconfig",
        ]
        .map(String::from)
        .into()
    }

    #[test]
    fn private_options_preserve_wrapper_identity_and_proxy_arguments() -> Result<()> {
        assert_eq!(
            sanitize(&wrapper_arguments())?,
            [
                "-B",
                "fixture.py",
                "state.json",
                "-i",
                "-A",
                "-Jjump",
                "-o",
                "ProxyCommand=ssh -W %h:%p jump",
                "-v",
                "-Fconfig",
            ]
        );
        for argument in ["-fnA", "-N", "-Wpeer:22", "-Oexit", "-G", "-Z", "-s", "--"] {
            assert!(sanitize(&[argument.into()]).is_err(), "{argument}");
        }
        assert!(sanitize(&["-i".into()]).is_err());
        Ok(())
    }

    fn resolved(master: bool, user: &[&str]) -> Result<String> {
        let mut command: Vec<String> = ["ssh", "-F", "/dev/null"].map(String::from).into();
        command.extend(user.iter().map(|value| value.to_string()));
        command.extend(["--", "example.invalid", ":"].map(String::from));
        let mut values = vec![
            "-o".into(),
            format!("ControlMaster={}", if master { "yes" } else { "no" }),
            "-o".into(),
            "ControlPath=/tmp/csb-owned-regression".into(),
        ];
        if master {
            values.insert(0, "-M".into());
        }
        let mut command = super::super::options(&command, &values)?;
        command.insert(1, "-G".into());
        let output = Command::new(&command[0]).args(&command[1..]).output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8(output.stdout)?)
    }

    fn assert_private(config: &str, master: bool) {
        for expected in [
            "controlpath /tmp/csb-owned-regression",
            if master {
                "controlmaster true"
            } else {
                "controlmaster false"
            },
            "forkafterauthentication no",
            "forwardagent no",
            "clearallforwardings yes",
            "controlpersist no",
            "stdinnull no",
            "sessiontype default",
        ] {
            assert!(
                config.lines().any(|line| line == expected),
                "{expected}\n{config}"
            );
        }
        assert!(!config.lines().any(|line| line.starts_with("localforward ")));
        if let Some(command) = config.lines().find(|line| line.starts_with("remotecommand ")) {
            assert_eq!(command, "remotecommand none");
        }
    }

    #[test]
    fn openssh_resolves_private_lifecycle_options_over_user_settings() -> Result<()> {
        let cases: &[&[&str]] = &[
            &[
                "-o",
                "ControlPath=none",
                "-oControlMaster=auto",
                "-o",
                "ForkAfterAuthentication=yes",
            ],
            &["-S", "/tmp/user-mux", "-MMfA", "-L43210:localhost:1234"],
            &["-S/tmp/user-mux", "-o", "cOnTrOlPaTh /tmp/other"],
            &[
                "-o",
                "SessionType=none",
                "-oStdinNull=yes",
                "-oForwardAgent=yes",
                "-oRemoteCommand=echo fixture",
            ],
        ];
        for master in [true, false] {
            for user in cases {
                assert_private(&resolved(master, user)?, master);
            }
        }
        Ok(())
    }
}
