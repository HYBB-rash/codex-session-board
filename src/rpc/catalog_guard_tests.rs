use super::*;
use crate::rpc::{Role, Rpc, identity_fixture::Fixture};

fn guard() -> Result<CatalogGuard> {
    let mut guard = CatalogGuard::initialized(&json!({
        "codexHome":"/remote/.codex", "userAgent":"session_board_runtime/0.169.0 (Linux)"
    }))?;
    guard.bind(Path::new("/remote/.codex"), json!({"peer":"verified"}))?;
    Ok(guard)
}

#[test]
fn catalog_requires_bound_initialization_and_reports_verified_source() -> Result<()> {
    for home in [Value::Null, json!("relative"), json!("/remote/../other"), json!("/bad\nhome")] {
        assert!(CatalogGuard::initialized(&json!({"codexHome":home})).is_err());
    }
    let mut unbound = CatalogGuard::initialized(&json!({"codexHome":"/remote/.codex"}))?;
    assert!(unbound.source().is_err());
    assert!(unbound.check("project/list", &json!({})).is_err());
    assert!(unbound.bind(Path::new("/other"), json!({})).is_err());
    let source = guard()?.source()?;
    assert_eq!(source["codexHome"], "/remote/.codex");
    assert_eq!(source["version"], "0.169.0");
    assert_eq!(source["peerFingerprint"].as_str().unwrap().len(), 64);
    assert_eq!(source, guard()?.source()?);
    Ok(())
}

#[test]
fn catalog_restricts_database_reads_metadata_account_and_exact_state_path() -> Result<()> {
    let guard = guard()?;
    for (method, field, value) in [
        ("thread/list", "useStateDbOnly", true),
        ("thread/read", "includeTurns", false),
        ("account/read", "refreshToken", false),
    ] {
        guard.check(method, &json!({field:value}))?;
        for invalid in [json!({}), json!({field:!value}), json!({field:null}), json!({field:"false"})] {
            assert!(guard.check(method, &invalid).is_err());
        }
    }
    for method in ["fs/getMetadata", "fs/readFile"] {
        guard.check(method, &json!({"path":"/remote/.codex/.codex-global-state.json"}))?;
        for path in ["/other/.codex-global-state.json", "/remote/.codex/auth.json",
            "/remote/.codex/../.codex-global-state.json", "/remote/.codex/.codex-global-state.json/"] {
            assert!(guard.check(method, &json!({"path":path})).is_err());
        }
        assert!(guard.check(method, &json!({})).is_err());
    }
    assert!(guard.check("project/list", &Value::Null).is_err());
    Ok(())
}

#[test]
fn catalog_role_denies_all_execution_writes_history_and_watches() -> Result<()> {
    for method in ["initialize", "project/list", "project/read", "thread/list",
        "thread/read", "threadSection/list", "account/read", "fs/getMetadata", "fs/readFile"] {
        Role::CatalogReader.check(method)?;
    }
    for method in ["project/import", "project/create", "project/update", "thread/metadata/update",
        "thread/start", "thread/resume", "turn/start", "thread/name/set", "thread/archive",
        "thread/turns/list", "fs/watch", "fs/writeFile", "command/exec"] {
        assert!(Role::CatalogReader.check(method).is_err());
    }
    Ok(())
}

#[test]
fn verified_remote_catalog_rejects_unsafe_requests_before_transport() -> Result<()> {
    let fixture = Fixture::new()?;
    let app = fixture.app()?;
    let mut connection = app.connection(std::time::Duration::from_secs(3), Role::CatalogReader)?;
    assert_eq!(connection.catalog_source()?["codexHome"], fixture.peer()?.codex_home);
    let before = fixture.events();
    for (method, params) in [
        ("thread/read", json!({"includeTurns":true})),
        ("thread/list", json!({})),
        ("account/read", json!({"refreshToken":true})),
        ("fs/readFile", json!({"path":"/remote/auth.json"})),
        ("thread/start", json!({})),
    ] {
        assert!(connection.call(method, params).is_err());
    }
    assert_eq!(fixture.events(), before);
    Ok(())
}

#[test]
fn verified_remote_catalog_accepts_only_metadata_and_its_own_state_file() -> Result<()> {
    let fixture = Fixture::new()?;
    let app = fixture.app()?;
    let mut connection = app.connection(std::time::Duration::from_secs(3), Role::CatalogReader)?;
    let path = Path::new(&fixture.peer()?.codex_home).join(".codex-global-state.json");
    for (method, params) in [
        ("project/list", json!({})), ("project/read", json!({"projectId":"project"})),
        ("thread/list", json!({"useStateDbOnly":true})), ("threadSection/list", json!({})),
        ("thread/read", json!({"threadId":"thread","includeTurns":false})),
        ("account/read", json!({"refreshToken":false})),
        ("fs/getMetadata", json!({"path":path})), ("fs/readFile", json!({"path":path})),
    ] {
        assert_eq!(connection.call(method, params)?, json!({"ok":true}));
    }
    Ok(())
}

#[test]
fn local_home_is_derived_from_owned_absolute_directories() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let home = directory.path().canonicalize()?;
    std::fs::create_dir(home.join(".codex"))?;
    assert_eq!(local_home(&home, None)?, home.join(".codex"));
    assert_eq!(local_home(&home, Some(&home))?, home);
    assert!(local_home(Path::new("relative"), None).is_err());
    assert!(local_home(&home, Some(Path::new("relative"))).is_err());
    assert!(local_home(&home, Some(&home.join("missing"))).is_err());
    Ok(())
}
