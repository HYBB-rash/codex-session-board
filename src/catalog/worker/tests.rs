use super::*;
use crate::catalog::{test_support::*, view};

#[test]
fn only_ordinary_active_catalog_rows_enter_the_task_store() -> Result<()> {
    let (_directory, app) = app()?;
    let mut snapshot = snapshot();
    snapshot.active.push(thread(SECOND, false, false));
    snapshot.archived.push(thread(THIRD, true, true));
    publish(&app, snapshot)?;
    assert!(app.store.contains(FIRST)?);
    assert!(!app.store.has_session(SECOND)?);
    assert!(!app.store.has_session(THIRD)?);
    assert_eq!(app.store.catalog_published_ids()?.len(), 3);
    let view = view::read(&app, &[])?;
    assert_eq!(view.rows.len(), 3);
    assert_eq!(view.health["complete"], true);
    for row in &view.rows {
        assert_eq!(row["readOnly"], row["id"] != FIRST);
    }
    Ok(())
}

#[test]
fn nested_cursor_errors_restart_from_the_beginning_with_bounded_retries() -> Result<()> {
    let (_directory, app) = app()?;
    for attempt in 1..=3 {
        {
            let mut state = app.catalog.lock().unwrap();
            state.progress = Some(json!({"cursor":"stale"}));
            state.last_attempt = now();
            state.last_check = now();
        }
        failed(
            &app,
            anyhow::anyhow!("cursor expired").context("目录扫描尚未完成"),
        )?;
        let state = app.catalog.lock().unwrap();
        assert!(state.progress.is_none());
        assert_eq!(state.restarts, attempt);
        assert!(!state.live);
        assert!(state.error.as_deref().unwrap().contains("cursor expired"));
    }
    assert!(refresh::Check::due(&app, false)?.is_none());
    assert!(refresh::Check::due(&app, true)?.is_some());
    Ok(())
}

struct Reads(Vec<String>);

impl crate::rpc::Rpc for Reads {
    fn call(&mut self, method: &str, _params: serde_json::Value) -> Result<serde_json::Value> {
        self.0.push(method.into());
        Ok(match method {
            "account/read" => json!({}),
            "fs/getMetadata" => json!({"isFile":true,"isSymlink":false,"modifiedAtMs":1}),
            "fs/readFile" => json!({"dataBase64":"e30="}),
            _ => anyhow::bail!("unexpected {method}"),
        })
    }
}

#[test]
fn cached_or_failed_sources_require_full_input_revalidation() -> Result<()> {
    for failed in [false, true] {
        let (_directory, app) = app()?;
        let snapshot = snapshot();
        let source = snapshot.source.clone();
        install(&app, snapshot, false);
        {
            let mut state = app.catalog.lock().unwrap();
            state.last_check = now() - 40.0;
            state.error = failed.then(|| "temporary failure".into());
        }
        let check = refresh::Check::due(&app, false)?.unwrap();
        let mut reads = Reads(Vec::new());
        assert!(check.prepare(&app, &mut reads, source, false)?.is_some());
        assert_eq!(reads.0, ["account/read", "fs/getMetadata", "fs/readFile"]);
    }
    Ok(())
}
