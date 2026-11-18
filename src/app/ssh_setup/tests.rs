use super::*;
use crate::{config::Config, rpc::identity_fixture::Fixture};
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

fn target() -> Value {
    json!({"hostname":"mac.example","user":"remote","port":"22","directory":"/project"})
}

fn fixture(parent: &Path) -> Result<(Fixture, Config)> {
    let peer = Fixture::new()?;
    peer.update("results", json!({
        "account/read":{"userId":"test","workspaceRouting":{"chatgptAccountId":"isolated"}},
        "project/list":{"data":[{"id":"isolated","roots":[{"path":"/project"}]}],"nextCursor":null}
    }))?;
    let registry = parent.join("registry.json");
    fs::write(&registry, b"{}")?;
    let client = parent.join("open");
    fs::write(&client, "#!/bin/sh\nprintf '%s\\n' \"$1\" >> opens\n")?;
    fs::set_permissions(&client, fs::Permissions::from_mode(0o700))?;
    let config = Config { data_dir: parent.join("board"), sidebar_state_file: registry,
        open_command: vec![client.to_string_lossy().into_owned()], ..peer.config.clone() };
    Ok((peer, config))
}

fn preview(app: &App, directory: &Path) -> Result<Value> {
    app.ssh_setup_at(&json!({"action":"preview","target":target()}), directory)
}

#[test]
fn ssh_decline_survives_app_restart_and_a_dismissed_restore_preview() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let (_peer, config) = fixture(temporary.path())?;
    let directory = temporary.path().join(".ssh");
    let app = App::new(config.clone())?;
    let shown = preview(&app, &directory)?;
    assert_eq!(app.store.ssh_setup_records()?.len(), 0);
    let declined = app.ssh_setup_at(&json!({"action":"decline","previewId":shown["previewId"]}), &directory)?;
    assert_eq!(declined["state"], "declined");
    assert!(!directory.exists());
    drop(app);
    let app = App::new(config.clone())?;
    assert_eq!(preview(&app, &directory)?["state"], "declined");
    let restored = app.ssh_setup_at(&json!({"action":"restore","targetId":declined["targetId"]}), &directory)?;
    assert_eq!(restored["state"], "preview");
    assert_ne!(restored["previewId"], shown["previewId"]);
    drop(app);
    let app = App::new(config)?;
    let status = app.ssh_setup_at(&json!({"action":"status"}), &directory)?;
    assert_eq!(status["saved"][0]["state"], "declined");
    assert!(!directory.exists());
    Ok(())
}

#[test]
fn ssh_apply_checks_drift_writes_real_files_and_recovers_the_same_alias() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let (peer, config) = fixture(temporary.path())?;
    let directory = temporary.path().join(".ssh");
    fs::create_dir(&directory)?;
    let app = App::new(config.clone())?;
    let shown = preview(&app, &directory)?;
    let apply = |preview: &Value| json!({"action":"apply","previewId":preview["previewId"]});
    fs::write(directory.join("config"), "ServerAliveInterval 11\n")?;
    assert!(app.ssh_setup_at(&apply(&shown), &directory).is_err());
    assert!(!directory.join("codex-session-board.conf").exists());
    let shown = preview(&app, &directory)?;
    assert_eq!(app.ssh_setup_at(&apply(&shown), &directory)?["state"], "applied");
    assert!(fs::read_to_string(directory.join("config"))?.ends_with("Host *\nServerAliveInterval 11\n"));
    let fragment = fs::read_to_string(directory.join("codex-session-board.conf"))?;
    assert_eq!(fragment.matches("Host csb-mac\n").count(), 1);
    assert!(peer.events().lines().all(|line| !matches!(line, "thread/start" | "turn/start" | "project/import")));
    confirm_registry_replay(app, config, &directory, &apply(&shown))?;
    assert_eq!(fs::read_to_string(directory.join("codex-session-board.conf"))?, fragment);
    Ok(())
}

fn confirm_registry_replay(app: App, config: Config, directory: &Path, apply: &Value) -> Result<()> {
    let host = "remote-ssh-discovered:csb-mac";
    let registration = json!({
        "codex-managed-remote-connections":[{"hostId":host,"displayName":"csb-mac","alias":"csb-mac"}],
        "remote-connection-auto-connect-by-host-id":{(host):true},
        "remote-projects":[{"id":"project","hostId":host,"remotePath":"/project"}]
    });
    fs::write(&config.sidebar_state_file, serde_json::to_vec(&registration)?)?;
    drop(app);
    let app = App::new(config)?;
    assert_eq!(app.ssh_setup_at(apply, directory)?["state"], "registered");
    let opens = fs::read_to_string(app.config.data_dir.join("opens"))?;
    assert_eq!(opens.lines().count(), 1);
    assert!(opens.contains("name=csb-mac&projectPath=%2Fproject"));
    Ok(())
}

#[test]
fn ssh_apply_resumes_partial_registration_but_preserves_user_changes() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let (peer, config) = fixture(temporary.path())?;
    peer.update("rpcEvents", json!(true))?;
    let directory = temporary.path().join(".ssh");
    let app = App::new(config.clone())?;
    let shown = preview(&app, &directory)?;
    let apply = json!({"action":"apply","previewId":shown["previewId"]});
    assert_eq!(app.ssh_setup_at(&apply, &directory)?["state"], "applied");
    let host = "remote-ssh-discovered:csb-mac";
    let mut partial = json!({"codex-managed-remote-connections":[
        {"hostId":host,"displayName":"csb-mac","alias":"csb-mac"}]});
    fs::write(&config.sidebar_state_file, serde_json::to_vec(&partial)?)?;
    drop(app);
    let app = App::new(config)?;
    assert_eq!(app.ssh_setup_at(&apply, &directory)?["state"], "applied");
    let opens = app.config.data_dir.join("opens");
    let receipts = fs::read_to_string(&opens)?;
    assert_eq!(receipts.lines().count(), 2);
    assert_eq!(receipts.lines().next(), receipts.lines().last());
    partial["remote-connection-auto-connect-by-host-id"] = json!({(host):false});
    fs::write(&app.config.sidebar_state_file, serde_json::to_vec(&partial)?)?;
    assert!(app.ssh_setup_at(&apply, &directory).unwrap_err().to_string().contains("自动连接或目录"));
    assert_eq!(fs::read_to_string(opens)?, receipts);
    assert!(!peer.events().contains("thread/start"));
    Ok(())
}

#[test]
fn ssh_preview_rejects_old_connection_and_dynamic_config_without_writing() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let (peer, config) = fixture(temporary.path())?;
    let directory = temporary.path().join(".ssh");
    fs::create_dir(&directory)?;
    fs::write(directory.join("config"), "Match exec false\n")?;
    let app = App::new(config)?;
    assert!(preview(&app, &directory).unwrap_err().to_string().contains("需手动管理"));
    assert!(peer.events().is_empty());
    fs::write(directory.join("config"), "")?;
    let old = json!({"codex-managed-remote-connections":[{
        "hostId":"remote-ssh-codex-managed:old","displayName":"Mac","hostname":"remote@mac.example","alias":null}]});
    let old = serde_json::to_vec(&old)?;
    fs::write(&app.config.sidebar_state_file, &old)?;
    assert!(preview(&app, &directory).unwrap_err().to_string().contains("请使用原连接"));
    assert_eq!(fs::read(&app.config.sidebar_state_file)?, old);
    assert!(!directory.join("codex-session-board.conf").exists());
    assert_eq!(app.store.ssh_setup_records()?.len(), 0);
    Ok(())
}

#[test]
fn ssh_preview_reuses_a_user_selected_existing_alias_without_file_changes() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let (_peer, mut config) = fixture(temporary.path())?;
    let directory = temporary.path().join(".ssh");
    fs::create_dir(&directory)?;
    let parsed = Target::parse(&target())?;
    let text = format!("{}{}", parsed.block("work-mac"), parsed.block("studio-mac"));
    fs::write(directory.join("config"), &text)?;
    config.ssh_command = vec!["ssh".into()];
    let open_ssh = Probe::new(&config, &text)?;
    let direct = open_ssh.settings(&host(&parsed, None)?)?;
    assert_eq!(matching_aliases(&open_ssh, &parsed, &direct, &Files::declares_aliases(&text))?,
        ["studio-mac", "work-mac"]);
    assert!(Files::read(&directory)?.fragment.is_none());
    assert_eq!(fs::read_to_string(directory.join("config"))?, text);
    Ok(())
}
