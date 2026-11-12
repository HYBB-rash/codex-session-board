use super::*;
use rusqlite::{Connection, OpenFlags};

pub(super) fn before_start(path: &Path, settings: &Config) -> Result<()> {
    let identity = info(settings)?;
    let destination = settings.data_dir.join(".backups").join(format!(
        "{}-{}", identity["build"].as_str().context("缺少构建身份")?,
        identity["configId"].as_str().context("缺少配置身份")?
    ));
    if destination.try_exists()? {
        let previous: Value = serde_json::from_slice(&fs::read(destination.join("identity.json"))?)?;
        ensure!(previous["build"] == identity["build"] && previous["configId"] == identity["configId"],
            "运行时备份身份不符");
        return Ok(());
    }
    let parent = destination.parent().context("备份缺少父目录")?;
    config::create_private_dir(parent)?;
    let staging = parent.join(format!(".pending-{}", uuid::Uuid::new_v4()));
    config::create_private_dir(&staging)?;
    let result = snapshot(path, settings, &staging, &identity)
        .and_then(|()| fs::rename(&staging, &destination).map_err(Into::into));
    if result.is_err() {
        fs::remove_dir_all(&staging).context("运行时备份失败且临时目录未能清理")?;
    }
    result
}

fn databases(data: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = vec![data.join("status.sqlite3")];
    let remotes = data.join("remote-hosts");
    if remotes.try_exists()? {
        for entry in fs::read_dir(remotes)? {
            let entry = entry?;
            ensure!(entry.file_type()?.is_dir(), "远程缓存路径不是普通目录");
            paths.push(entry.path().join("status.sqlite3"));
        }
    }
    Ok(paths)
}

fn snapshot(path: &Path, settings: &Config, destination: &Path, identity: &Value) -> Result<()> {
    config::write_json(&destination.join("effective-config.json"),
        &serde_json::to_value(settings)?, false)?;
    fs::copy(path, destination.join("config.json"))?;
    for source in databases(&settings.data_dir)? {
        if !source.try_exists()? {
            continue;
        }
        let relative = source.strip_prefix(&settings.data_dir)?;
        let target = destination.join(relative);
        config::create_private_dir(target.parent().context("备份数据库缺少父目录")?)?;
        ensure!(migration::import_database(&source, &target)?, "运行时来源库消失");
        let copied = Connection::open_with_flags(&target, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let integrity: String = copied.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        ensure!(integrity == "ok", "运行时备份完整性检查失败：{integrity}");
    }
    config::write_json(&destination.join("identity.json"), identity, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_backup_preserves_wal_notes_disabled_hosts_and_configuration() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let data = directory.path().join("data");
        let settings = Config { data_dir: data.clone(), ..Config::default() };
        let path = directory.path().join("config.json");
        let bytes = serde_json::to_vec(&settings)?;
        fs::write(&path, &bytes)?;
        let mut originals = Vec::new();
        for relative in ["", "remote-hosts/disabled"] {
            let root = data.join(relative);
            config::create_private_dir(&root)?;
            let database = Connection::open(root.join("status.sqlite3"))?;
            database.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE sessions(id TEXT,note TEXT);
                INSERT INTO sessions VALUES('same-id','retain this note');")?;
            originals.push(database);
        }
        fs::write(data.join("private-key"), "never copy unrelated files")?;
        before_start(&path, &settings)?;
        let backup = fs::read_dir(data.join(".backups"))?.next().context("缺少备份")??.path();
        for relative in ["status.sqlite3", "remote-hosts/disabled/status.sqlite3"] {
            let copied = Connection::open(backup.join(relative))?;
            let row: (String, String) = copied.query_row("SELECT id,note FROM sessions", [],
                |row| Ok((row.get(0)?, row.get(1)?)))?;
            assert_eq!(row, ("same-id".into(), "retain this note".into()));
        }
        assert!(!backup.join("private-key").exists());
        assert_eq!(fs::read(&path)?, bytes);
        assert_eq!(fs::read(backup.join("config.json"))?, bytes);
        before_start(&path, &settings)?;
        assert_eq!(fs::read_dir(data.join(".backups"))?.count(), 1);
        Ok(())
    }
}
