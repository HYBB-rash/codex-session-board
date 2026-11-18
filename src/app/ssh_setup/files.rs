use anyhow::{Context, Result, ensure};
use rustix::fs::{Mode, OFlags};
use std::{
    collections::BTreeSet, fs::{self, File}, io::{Read, Write},
    os::unix::fs::{MetadataExt, PermissionsExt}, path::{Path, PathBuf},
};

const FRAGMENT: &str = "codex-session-board.conf";
const INCLUDE: &str = "Include ~/.ssh/codex-session-board.conf\n";
const HEADER: &str = "Include ~/.ssh/codex-session-board.conf\nHost *\n";
const LIMIT: u64 = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct Document {
    pub(super) text: String,
    mode: u32,
    inode: u64,
    device: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct Files {
    pub(super) directory: PathBuf,
    pub(super) main: Option<Document>,
    pub(super) fragment: Option<Document>,
}

impl Files {
    pub(super) fn read(directory: &Path) -> Result<Self> {
        if let Some(metadata) = metadata(directory)? {
            ensure!(metadata.is_dir() && metadata.uid() == rustix::process::getuid().as_raw()
                && metadata.mode() & 0o022 == 0, "SSH 配置目录的类型、所有者或权限不受支持");
        }
        Ok(Self { directory: directory.to_owned(), main: read(&directory.join("config"))?,
            fragment: read(&directory.join(FRAGMENT))? })
    }

    pub(super) fn expanded(&self) -> Result<String> {
        let main = text(&self.main);
        let fragment = text(&self.fragment);
        validate(main, true)?;
        validate(fragment, false)?;
        Ok(main.replacen(INCLUDE, &format!("{fragment}\n"), 1))
    }

    pub(super) fn declares_aliases(config: &str) -> BTreeSet<String> {
        config.lines().filter_map(|line| line.trim_start().split_once([' ', '\t', '=']))
            .filter(|(directive, _)| directive.eq_ignore_ascii_case("Host"))
            .flat_map(|(_, patterns)| patterns.trim_start_matches('=').split_whitespace()
                .take_while(|part| !part.starts_with('#')))
            .filter(|alias| super::target::name(alias))
            .map(str::to_owned).collect()
    }

    pub(super) fn paths(&self) -> [PathBuf; 2] {
        [self.directory.join("config"), self.directory.join(FRAGMENT)]
    }

    pub(super) fn changes(&self, block: &str) -> [Option<String>; 2] {
        let main = text(&self.main);
        [(!main.starts_with(INCLUDE)).then(|| format!("{HEADER}{main}")),
            Some(format!("{}{block}", terminated(text(&self.fragment))))]
    }

    pub(super) fn effective_after(&self, changes: &[Option<String>; 2]) -> String {
        let main = changes[0].as_deref().unwrap_or_else(|| text(&self.main));
        let fragment = changes[1].as_deref().unwrap_or_else(|| text(&self.fragment));
        main.replacen(INCLUDE, &format!("{fragment}\n"), 1)
    }

    pub(super) fn publish(&self, changes: &[Option<String>; 2]) -> Result<()> {
        crate::config::create_private_dir(&self.directory)?;
        let paths = self.paths();
        let originals = [&self.main, &self.fragment];
        for index in (0..paths.len()).rev() {
            if let Some(next) = &changes[index] {
                replace(&paths[index], originals[index], next)?;
            }
        }
        Ok(())
    }

    pub(super) fn check_writable(&self, changes: &[Option<String>; 2]) -> Result<()> {
        let files = [&self.main, &self.fragment];
        ensure!(files.iter().zip(changes).all(|(old, next)| next.is_none()
            || old.as_ref().is_none_or(|file| file.mode & 0o200 != 0)), "SSH 配置文件只读");
        Ok(())
    }

    pub(super) fn hashes(&self) -> [Option<String>; 2] {
        [&self.main, &self.fragment].map(|file| file.as_ref().map(|file| super::digest(file.text.as_bytes())))
    }

    pub(super) fn remaining(
        &self, before: &[Option<String>; 2], after: &[Option<String>; 2], block: Option<&str>,
    ) -> Result<[Option<String>; 2]> {
        let hashes = self.hashes();
        ensure!(hashes.iter().enumerate().all(|(index, hash)| hash == &before[index] || hash == &after[index]),
            "SSH 配置已被更改，请核对后重新预览");
        let mut changes = block.map_or([None,None], |block| self.changes(block));
        for index in 0..2 {
            if hashes[index] == after[index] {
                changes[index] = None;
            }
            if let Some(updated) = &changes[index] {
                ensure!(after[index].as_deref() == Some(super::digest(updated.as_bytes()).as_str()),
                    "SSH 配置与同意的改动不同");
            }
        }
        Ok(changes)
    }
}

fn validate(config: &str, include: bool) -> Result<()> {
    for (index, line) in config.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let directive = line.split([' ', '\t', '=']).next().unwrap_or_default().to_ascii_lowercase();
        if directive == "include" && include && index == 0 && config.starts_with(INCLUDE) {
            continue;
        }
        ensure!(matches!(directive.as_str(), "host" | "hostname" | "user" | "port"
            | "identityfile" | "identitiesonly" | "identityagent" | "userknownhostsfile"
            | "globalknownhostsfile" | "stricthostkeychecking" | "serveraliveinterval"
            | "serveralivecountmax" | "connecttimeout" | "addressfamily" | "loglevel")
            && !line.ends_with('\\') && !line.chars().any(|character| character.is_control() && character != '\t'),
            "SSH 配置含需手动管理的指令 {directive}，请在 Codex 中配置连接");
        ensure!(!line.contains(['%', '$']), "SSH 配置含依赖原始主机名或环境的变量，请手动配置");
        ensure!(directive != "host" || !line.contains(['"', '\'', '\\']),
            "SSH Host 模式含引号或转义，请手动配置");
    }
    Ok(())
}

fn text(document: &Option<Document>) -> &str {
    document.as_ref().map(|file| file.text.as_str()).unwrap_or_default()
}

fn terminated(value: &str) -> String {
    if value.is_empty() || value.ends_with('\n') { value.to_owned() } else { format!("{value}\n") }
}

fn metadata(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn read(path: &Path) -> Result<Option<Document>> {
    let descriptor = match rustix::fs::open(path,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC, Mode::empty()) {
        Ok(descriptor) => descriptor,
        Err(rustix::io::Errno::NOENT) => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("无法读取 {}", path.display())),
    };
    let file = File::from(descriptor);
    let metadata = file.metadata()?;
    ensure!(metadata.is_file() && metadata.uid() == rustix::process::getuid().as_raw()
        && metadata.mode() & 0o022 == 0 && metadata.nlink() == 1, "SSH 配置文件所有者、权限或类型不受支持");
    let mut contents = String::new();
    file.take(LIMIT + 1).read_to_string(&mut contents)?;
    ensure!(contents.len() as u64 <= LIMIT, "SSH 配置超过预览大小限制");
    Ok(Some(Document { text: contents, mode: metadata.mode() & 0o777,
        inode: metadata.ino(), device: metadata.dev() }))
}

fn replace(path: &Path, expected: &Option<Document>, content: &str) -> Result<()> {
    let parent = path.parent().context("SSH 文件缺少父目录")?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.as_file().set_permissions(fs::Permissions::from_mode(
        expected.as_ref().map_or(0o600, |file| file.mode)))?;
    staged.write_all(content.as_bytes())?;
    staged.as_file().sync_all()?;
    let current = read(path)?;
    if current.as_ref().is_some_and(|file| file.text == content) {
        return Ok(());
    }
    ensure!(&current == expected, "SSH 配置已被更改，请重新预览");
    if current.is_some() {
        staged.persist(path)?;
    } else {
        staged.persist_noclobber(path)?;
    }
    File::open(parent)?.sync_all()?;
    Ok(())
}
