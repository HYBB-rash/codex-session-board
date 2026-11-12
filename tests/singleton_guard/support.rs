use anyhow::Result;
use std::fs;
use std::os::unix::{
    fs::{PermissionsExt, symlink},
    net::UnixListener,
};
use std::path::PathBuf;
use tempfile::TempDir;

pub struct Fixture {
    pub root: TempDir,
    pub profile: PathBuf,
    pub temporary: PathBuf,
    pub listener: UnixListener,
}

impl Fixture {
    pub fn new() -> Result<Self> {
        let root = tempfile::Builder::new()
            .prefix("csb-singleton-")
            .tempdir_in("/tmp")?;
        let profile = root.path().join("profile");
        let temporary = root.path().join("temporary");
        for path in [&profile, &temporary] {
            fs::create_dir(path)?;
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
        }
        let socket = temporary.join("SingletonSocket");
        let listener = UnixListener::bind(&socket)?;
        let hostname = fs::read_to_string("/proc/sys/kernel/hostname")?;
        symlink(
            format!("{}-{}", hostname.trim(), std::process::id()),
            profile.join("SingletonLock"),
        )?;
        symlink(&socket, profile.join("SingletonSocket"))?;
        for directory in [&profile, &temporary] {
            symlink("123456789", directory.join("SingletonCookie"))?;
        }
        Ok(Self {
            root,
            profile,
            temporary,
            listener,
        })
    }
}
