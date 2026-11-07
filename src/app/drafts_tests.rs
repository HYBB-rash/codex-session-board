use super::*;
use std::collections::VecDeque;

struct Reader {
    responses: VecDeque<Result<Value>>,
    calls: Vec<(String, Value)>,
}

fn isolated() -> Result<(tempfile::TempDir, App)> {
    let directory = tempfile::tempdir()?;
    let app = App::new(Config {
        data_dir: directory.path().into(),
        sync_titles: false,
        ..Config::default()
    })?;
    Ok((directory, app))
}

impl Rpc for Reader {
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.calls.push((method.into(), params));
        self.responses.pop_front().context("非预期 RPC 调用")?
    }
}

impl Reader {
    fn methods(&self) -> Vec<&str> {
        self.calls.iter().map(|call| call.0.as_str()).collect()
    }
}

fn draft<'app>(remote: &'app App, request: &str) -> Result<Draft<'app>> {
    let (record, fresh) = remote.store.draft_begin("remote:project", request)?;
    Ok(Draft {
        remote,
        project: json!({"id":"remote:project","paths":["/remote/root"]}),
        record,
        request: request.into(),
        fresh,
    })
}

#[test]
fn lost_empty_draft_receipts_never_replay_thread_start() -> Result<()> {
    assert!(draft_params(&json!({"paths":["relative"]})).is_err());
    assert!(draft_params(&json!({"paths":[]})).is_err());
    let (_directory, app) = isolated()?;
    let request = uuid::Uuid::new_v4().to_string();
    let mut reader = Reader {
        responses: VecDeque::from([Err(anyhow::anyhow!("回执丢失"))]),
        calls: Vec::new(),
    };
    for _attempt in 0..2 {
        assert!(
            draft(&app, &request)?
                .thread(&app.store, &mut reader)
                .is_err()
        );
    }
    assert_eq!(reader.calls.len(), 1);
    assert_eq!(reader.calls[0].0, "thread/start");
    assert!(reader.calls[0].1.get("input").is_none());
    assert!(
        app.store
            .draft_begin("another-project", &uuid::Uuid::new_v4().to_string())?
            .1
    );
    Ok(())
}

#[test]
fn acknowledged_empty_drafts_only_read_the_same_thread_on_retry() -> Result<()> {
    let (_directory, app) = isolated()?;
    let request = uuid::Uuid::new_v4().to_string();
    let identifier = uuid::Uuid::new_v4().to_string();
    let wrong = uuid::Uuid::new_v4().to_string();
    let mut reader = Reader {
        responses: VecDeque::from([
            Ok(json!({"thread":{"id":identifier,"turns":[]}})),
            Ok(json!({"thread":{"id":wrong,"turns":[]}})),
            Ok(json!({"thread":{"id":identifier,"turns":[{"id":"user-turn"}]}})),
            Ok(json!({"thread":{"id":identifier,"turns":[]}})),
        ]),
        calls: Vec::new(),
    };
    assert_eq!(
        draft(&app, &request)?.thread(&app.store, &mut reader)?["id"],
        identifier
    );
    for _attempt in 0..2 {
        assert!(
            draft(&app, &request)?
                .thread(&app.store, &mut reader)
                .is_err()
        );
    }
    assert_eq!(
        draft(&app, &request)?.thread(&app.store, &mut reader)?["id"],
        identifier
    );
    assert_eq!(
        reader.methods(),
        ["thread/start", "thread/read", "thread/read", "thread/read"]
    );
    assert!(
        reader
            .calls
            .iter()
            .skip(1)
            .all(|call| call.1["threadId"] == identifier && call.1["includeTurns"] == true)
    );
    Ok(())
}
