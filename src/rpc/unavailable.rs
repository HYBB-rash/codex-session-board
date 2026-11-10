use super::*;

#[derive(Debug)]
pub(super) enum Unavailable {
    Desktop,
    Sidebar,
}

impl fmt::Display for Unavailable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let capability = match self {
            Self::Desktop => "Codex 执行状态连接不可用",
            Self::Sidebar => "Codex 侧栏连接不可用",
        };
        write!(
            formatter,
            "{capability}。请等正在运行的任务结束后，完整退出并重新打开 Codex，再刷新看板。详细步骤见“连接帮助”。"
        )
    }
}

impl std::error::Error for Unavailable {}

pub(super) fn stale(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<Errno>()
        .is_some_and(|error| matches!(*error, Errno::CONNREFUSED | Errno::NOENT))
        || error.downcast_ref::<io::Error>().is_some_and(|error| {
            matches!(
                error.kind(),
                io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
            )
        })
}

pub(super) fn explain(error: anyhow::Error, endpoint: Unavailable) -> anyhow::Error {
    if stale(&error) {
        error.context(endpoint)
    } else {
        error
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, Permissions};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;

    #[test]
    fn missing_desktop_socket_explains_recovery_and_preserves_the_cause() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let missing = directory.path().join("missing.sock");
        let alias = directory.path().join("missing-link.sock");
        std::os::unix::fs::symlink(&missing, &alias)?;
        for path in [&missing, &alias] {
            for role in [
                Role::Reader,
                Role::NativeReader,
                Role::TitleWriter,
                Role::DraftWriter,
                Role::ArchiveWriter,
            ] {
                let error = DesktopConnection::connect(path, Duration::from_millis(100), role)
                    .err()
                    .unwrap();
                assert!(error.downcast_ref::<Unavailable>().is_some());
                assert_eq!(
                    error.downcast_ref::<io::Error>().unwrap().kind(),
                    io::ErrorKind::NotFound
                );
                assert!(error.to_string().contains("完整退出并重新打开 Codex"));
                assert!(!error.to_string().contains("os error"));
                assert!(format!("{error:#}").contains(path.to_str().unwrap()));
            }
        }
        Ok(())
    }

    #[test]
    fn refused_desktop_socket_has_the_same_recovery_without_hiding_security_errors() -> Result<()> {
        let directory = tempfile::tempdir()?;
        fs::set_permissions(directory.path(), Permissions::from_mode(0o700))?;
        let path = directory.path().join("stale.sock");
        let listener = UnixListener::bind(&path)?;
        fs::set_permissions(&path, Permissions::from_mode(0o600))?;
        drop(listener);
        let error = DesktopConnection::connect(&path, Duration::from_millis(100), Role::Reader)
            .err()
            .unwrap();
        assert!(
            error.to_string().contains("执行状态连接不可用"),
            "{error:#}"
        );
        assert_eq!(error.downcast_ref::<Errno>(), Some(&Errno::CONNREFUSED));
        fs::set_permissions(&path, Permissions::from_mode(0o666))?;
        let error = DesktopConnection::connect(&path, Duration::from_millis(100), Role::Reader)
            .err()
            .unwrap();
        assert!(error.downcast_ref::<Unavailable>().is_none());
        assert!(error.to_string().contains("所有者或权限无效"));
        Ok(())
    }

    #[test]
    fn missing_sidebar_socket_explains_recovery_and_rediscovery_still_works() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let config = crate::config::Config {
            sidebar_socket: directory.path().join("missing.sock"),
            ..crate::config::Config::default()
        };
        let error = SidebarConnection::connect(&config, Duration::from_millis(100))
            .err()
            .unwrap();
        assert!(error.downcast_ref::<Unavailable>().is_some());
        assert!(error.to_string().contains("Codex 侧栏连接不可用"));
        assert!(error.to_string().contains("完整退出并重新打开 Codex"));
        assert!(format!("{error:#}").contains("missing.sock"));
        let path = directory.path().join("recreated.sock");
        let _listener = UnixListener::bind(&path)?;
        fs::set_permissions(&path, Permissions::from_mode(0o600))?;
        let _connection = SidebarConnection::connect(&config, Duration::from_millis(100))?;
        Ok(())
    }

    #[test]
    fn unrelated_io_failures_do_not_suggest_restarting_codex() {
        for kind in [
            io::ErrorKind::PermissionDenied,
            io::ErrorKind::TimedOut,
            io::ErrorKind::InvalidData,
        ] {
            let error = explain(
                io::Error::new(kind, "original failure").into(),
                Unavailable::Desktop,
            );
            assert!(error.downcast_ref::<Unavailable>().is_none());
            assert_eq!(error.to_string(), "original failure");
        }
    }
}
