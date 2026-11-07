use super::*;

const REMOTE_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS remote_sessions (
 id TEXT PRIMARY KEY, host_id TEXT NOT NULL, thread_id TEXT NOT NULL,
 host_label TEXT NOT NULL, title TEXT NOT NULL, cwd TEXT NOT NULL,
 project_id TEXT, updated_at INTEGER NOT NULL, state TEXT NOT NULL DEFAULT 'unknown',
 note TEXT NOT NULL DEFAULT '', version INTEGER NOT NULL DEFAULT 0,
 input_version INTEGER NOT NULL DEFAULT 0,
 UNIQUE(host_id,thread_id));
CREATE TABLE IF NOT EXISTS remote_projects (
 id TEXT PRIMARY KEY, value TEXT NOT NULL);
";

pub fn identity(host: &str, thread: &str) -> String {
    format!("remote:{}", json!(["codex", host, thread]))
}

impl Store {
    pub(super) fn upgrade_remote(database: &Connection) -> Result<()> {
        database.execute_batch(REMOTE_SCHEMA)?;
        Ok(())
    }

    pub fn remote_projects(&self) -> Result<Vec<Value>> {
        let database = self.connection()?;
        let mut statement = database.prepare("SELECT value FROM remote_projects ORDER BY id")?;
        let values = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        values
            .iter()
            .map(|value| serde_json::from_str(value).map_err(Into::into))
            .collect()
    }

    pub fn cache_remote_projects(&self, projects: &[Value]) -> Result<()> {
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        transaction.execute("DELETE FROM remote_projects", [])?;
        for project in projects {
            transaction.execute(
                "INSERT INTO remote_projects(id,value) VALUES (?,?)",
                params![text(project, "id"), project.to_string()],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn discover_remote(&self, host: &str, label: &str, threads: &[Value]) -> Result<()> {
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        for thread in threads {
            let identifier = text(thread, "id");
            let uuid = uuid::Uuid::parse_str(identifier).context("远程会话 ID 无效")?;
            ensure!(uuid.to_string() == identifier, "远程会话 ID 无效");
            ensure!(thread["hostId"] == host, "远程会话主机不匹配");
            let title: String = text(thread, "title").chars().take(300).collect();
            transaction.execute(
                "INSERT INTO remote_sessions(id,host_id,thread_id,host_label,title,cwd,project_id,updated_at)
                 VALUES (?,?,?,?,?,?,?,?) ON CONFLICT(host_id,thread_id) DO UPDATE SET
                 host_label=excluded.host_label,title=excluded.title,cwd=excluded.cwd,
                 project_id=excluded.project_id,
                 input_version=input_version+(updated_at<>excluded.updated_at),
                 updated_at=excluded.updated_at WHERE excluded.updated_at>=remote_sessions.updated_at",
                params![
                    identity(host, identifier), host, identifier, label, title,
                    text(thread, "cwd"), thread["projectId"].as_str(),
                    thread["updatedAt"].as_i64().context("远程会话缺少更新时间")?
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn remote_session(&self, identifier: &str) -> Result<Value> {
        self.connection()?
            .query_row(
                "SELECT * FROM remote_sessions WHERE id=?",
                [identifier],
                row_value,
            )
            .optional()?
            .context("远程会话不在看板中")
    }

    pub fn remote_sessions(&self) -> Result<Vec<Value>> {
        let database = self.connection()?;
        let mut statement =
            database.prepare("SELECT * FROM remote_sessions ORDER BY updated_at DESC,id")?;
        let rows = statement
            .query_map([], row_value)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows.iter().map(|row| json!({
            "id":row["id"],"threadId":row["thread_id"],"hostId":row["host_id"],
            "hostLabel":row["host_label"],"remote":true,"title":row["title"],"cwd":row["cwd"],
            "updatedAt":row["updated_at"],"state":row["state"],"note":row["note"],
            "manual":row["version"].as_i64().unwrap_or(0)>0,"source":"manual",
            "reason":"远程状态由本看板人工维护，尚未接入自动状态。",
            "version":row["version"],"inputVersion":row["input_version"],
            "projectId":row["project_id"].as_str().map(|project| identity(text(row,"host_id"),project)),
            "sectionVersion":null,"titleSyncPending":false,"canArchive":false
        })).collect())
    }

    pub fn save_remote(&self, body: &Value) -> Result<Value> {
        let identifier = body["id"].as_str().context("缺少会话 ID")?;
        let state = body["state"].as_str().context("远程会话需要人工选择状态")?;
        ensure!(
            crate::model::board_states().any(|(value, _)| value == state),
            "未知状态"
        );
        let mut database = self.connection()?;
        let transaction = database.transaction()?;
        let row = transaction
            .query_row(
                "SELECT * FROM remote_sessions WHERE id=?",
                [identifier],
                row_value,
            )
            .optional()?
            .context("远程会话不在看板中")?;
        let note = body
            .get("note")
            .unwrap_or(&row["note"])
            .as_str()
            .context("备注必须是文本")?
            .trim();
        ensure!(note.chars().count() <= 2000, "备注不能超过 2000 字");
        ensure!(state != "blocked" || !note.is_empty(), "请填写阻塞原因");
        let version = body["version"].as_i64().context("缺少状态版本")?;
        let input = body["inputVersion"].as_i64().context("缺少输入版本")?;
        ensure!(row["version"] == version, "状态已更新，请刷新后重试");
        ensure!(
            row["input_version"] == input,
            "远程会话已更新，请刷新并核对后重试"
        );
        transaction.execute(
            "UPDATE remote_sessions SET state=?,note=?,version=version+1 WHERE id=?",
            params![state, note, identifier],
        )?;
        transaction.execute(
            "INSERT INTO changes(thread_id,changed_at,state,note) VALUES (?,?,?,?)",
            params![identifier, now(), state, note],
        )?;
        transaction.commit()?;
        Ok(json!({"ok":true,"titleEnabled":false,"titlePending":false,"sectionsEnabled":false}))
    }
}
