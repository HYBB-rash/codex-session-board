use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

fn source_path(config: &crate::config::Config) -> Result<PathBuf> {
    if !config.sidebar_state_file.as_os_str().is_empty() {
        ensure!(
            config.sidebar_state_file.is_absolute(),
            "项目源必须为绝对路径"
        );
        return Ok(config.sidebar_state_file.clone());
    }
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".codex")))
        .context("无法确定 Codex 项目源，请配置 sidebar_state_file")?;
    ensure!(home.is_absolute(), "Codex 状态目录必须为绝对路径");
    Ok(home.join(".codex-global-state.json"))
}

pub fn refresh(app: &crate::app::App) -> Result<()> {
    let response = (|| {
        let path = source_path(&app.config)?;
        serde_json::from_reader(std::fs::File::open(path)?).context("无法读取完整 Codex 项目登记表")
    })();
    app.projects
        .lock()
        .map_err(|_| anyhow::anyhow!("项目列表锁已损坏"))?
        .update(response)
}

#[derive(Clone, Serialize)]
pub struct Project {
    pub id: String,
    pub label: String,
    pub paths: Vec<PathBuf>,
    #[serde(skip)]
    roots: Vec<PathBuf>,
}

#[derive(Default, Serialize)]
pub struct Catalog {
    pub loaded: bool,
    pub error: Option<String>,
    pub items: Vec<Project>,
    #[serde(skip)]
    assignments: HashMap<String, Value>,
}

fn normalized(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        let mut result = PathBuf::new();
        for component in path.components() {
            if component == Component::ParentDir {
                result.pop();
            } else {
                result.push(component.as_os_str());
            }
        }
        for ancestor in result.ancestors() {
            if let (Ok(root), Ok(suffix)) = (ancestor.canonicalize(), result.strip_prefix(ancestor))
            {
                return root.join(suffix);
            }
        }
        result
    })
}

fn project(id: &str, entry: &Value) -> Result<Project> {
    ensure!(entry["id"] == id, "项目身份不一致");
    let label = entry["name"].as_str().context("项目缺少名称")?;
    let paths = entry["rootPaths"]
        .as_array()
        .context("项目缺少目录")?
        .iter()
        .map(|path| path.as_str().map(PathBuf::from).context("项目目录无效"))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        !id.trim().is_empty() && !label.trim().is_empty(),
        "项目身份或名称为空"
    );
    ensure!(
        !paths.is_empty() && paths.iter().all(|path| path.is_absolute()),
        "项目需要绝对目录"
    );
    let roots = paths.iter().map(|path| normalized(path)).collect();
    Ok(Project {
        id: id.into(),
        label: label.into(),
        paths,
        roots,
    })
}

fn parse(response: Value) -> Result<Vec<Project>> {
    let entries = response["local-projects"]
        .as_object()
        .context("缺少完整本地项目登记表")?;
    let mut items = Vec::new();
    let mut roots = HashSet::new();
    for (id, entry) in entries {
        let item = project(id, entry)?;
        for root in &item.roots {
            ensure!(roots.insert(root.clone()), "同一目录存在多个项目");
        }
        items.push(item);
    }
    if let Some(order) = response.get("project-order") {
        let order = order.as_array().context("项目顺序无效")?;
        items.sort_by_key(|item| {
            order
                .iter()
                .position(|id| id == &item.id)
                .unwrap_or(usize::MAX)
        });
    }
    Ok(items)
}

impl Catalog {
    pub fn update(&mut self, response: Result<Value>) -> Result<()> {
        match response.and_then(parse) {
            Ok(items) => {
                self.items = items;
                self.loaded = true;
                self.error = None;
                Ok(())
            }
            Err(error) => {
                self.error = Some(error.to_string());
                Err(error)
            }
        }
    }

    fn identify(&self, cwd: &str) -> Value {
        if !self.loaded || !Path::new(cwd).is_absolute() {
            return Value::Null;
        }
        let path = normalized(Path::new(cwd));
        self.items
            .iter()
            .flat_map(|item| item.roots.iter().map(move |root| (item, root)))
            .filter(|(_, root)| path.starts_with(root))
            .max_by_key(|(_, root)| root.components().count())
            .map_or(Value::Null, |(item, _)| json!(item.id))
    }

    fn assignment(&self, cwd: &str) -> Result<Value> {
        let direct = self.identify(cwd);
        if !direct.is_null() || !self.loaded || !Path::new(cwd).is_absolute() {
            return Ok(direct);
        }
        let path = normalized(Path::new(cwd));
        let Some(repository) = crate::workspaces::repository(&path)? else {
            return Ok(Value::Null);
        };
        let relative = path.strip_prefix(&repository.root)?;
        let candidates = self.linked_roots(&repository, relative)?;
        let depth = candidates.iter().map(|(depth, _)| *depth).max();
        let matches: HashSet<_> = candidates
            .into_iter()
            .filter(|(candidate, _)| Some(*candidate) == depth)
            .map(|(_, id)| id)
            .collect();
        ensure!(matches.len() <= 1, "工作树对应多个已加载项目，无法确定归属");
        Ok(matches
            .into_iter()
            .next()
            .map_or(Value::Null, |id| json!(id)))
    }

    fn linked_roots(
        &self,
        repository: &crate::workspaces::Repository,
        relative: &Path,
    ) -> Result<Vec<(usize, String)>> {
        let mut candidates = Vec::new();
        for item in &self.items {
            for root in &item.roots {
                let Some(source) = crate::workspaces::repository(root)? else {
                    continue;
                };
                let suffix = root.strip_prefix(&source.root)?;
                if source.common == repository.common && relative.starts_with(suffix) {
                    candidates.push((suffix.components().count(), item.id.clone()));
                }
            }
        }
        Ok(candidates)
    }

    pub fn annotate(&mut self, sessions: &mut [Value]) {
        let mut paths = HashMap::new();
        for session in sessions {
            let cwd = session["cwd"].as_str().unwrap_or("").to_owned();
            let (identifier, issue) =
                paths
                    .entry(cwd.clone())
                    .or_insert_with(|| match self.assignment(&cwd) {
                        Ok(identifier) => (identifier, Value::Null),
                        Err(error) => {
                            let previous = self.assignments.get(&cwd).filter(|id| {
                                self.items
                                    .iter()
                                    .any(|item| id.as_str() == Some(item.id.as_str()))
                            });
                            (
                                previous.cloned().unwrap_or(Value::Null),
                                json!(error.to_string()),
                            )
                        }
                    });
            session["projectId"] = identifier.clone();
            session["projectIssue"] = issue.clone();
        }
        self.assignments = paths.into_iter().map(|(cwd, (id, _))| (cwd, id)).collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn local(id: &str, label: &str, path: &str) -> Value {
        json!({"id":id,"name":label,"rootPaths":[path]})
    }

    fn response(items: Vec<Value>) -> Result<Value> {
        let entries: serde_json::Map<_, _> = items
            .into_iter()
            .map(|item| (item["id"].as_str().unwrap().to_owned(), item))
            .collect();
        Ok(json!({"local-projects":entries,"project-order":["inner","outer"]}))
    }

    #[test]
    fn directory_membership_tracks_removal_and_readdition_without_mutating_saved_fields()
    -> Result<()> {
        let mut catalog = Catalog::default();
        let mut sessions = vec![
            json!({"id":"chat","cwd":"/repo/task","note":"保留","state":"review","version":7}),
        ];
        catalog.update(response(vec![local("first", "项目", "/repo")]))?;
        catalog.annotate(&mut sessions);
        assert_eq!(sessions[0]["projectId"], "first");
        catalog.update(response(vec![]))?;
        catalog.annotate(&mut sessions);
        assert!(sessions[0]["projectId"].is_null());
        catalog.update(response(vec![local("readded", "新名称", "/repo")]))?;
        catalog.annotate(&mut sessions);
        assert_eq!(sessions[0]["projectId"], "readded");
        assert_eq!(sessions[0]["note"], "保留");
        assert_eq!(sessions[0]["state"], "review");
        assert_eq!(sessions[0]["version"], 7);
        Ok(())
    }

    #[test]
    fn nested_roots_use_components_and_only_registered_local_projects() -> Result<()> {
        let mut catalog = Catalog::default();
        catalog.update(response(vec![
            local("outer", "同名", "/repo"),
            local("inner", "同名", "/repo/nested"),
            local("empty", "空项目", "/empty"),
        ]))?;
        assert_eq!(catalog.items.len(), 3);
        assert_eq!(catalog.identify("/repo/task"), "outer");
        assert_eq!(catalog.identify("/repo/nested/task"), "inner");
        assert!(catalog.identify("/repository").is_null());
        assert!(catalog.identify("/repo/../outside").is_null());
        assert!(catalog.identify("/outside").is_null());
        assert!(catalog.identify("").is_null());
        assert!(catalog.identify("repo").is_null());
        Ok(())
    }

    #[test]
    fn failed_or_malformed_refresh_retains_the_last_complete_catalog() -> Result<()> {
        let mut catalog = Catalog::default();
        assert!(!catalog.loaded);
        assert!(catalog.update(Err(anyhow::anyhow!("断连"))).is_err());
        assert!(!catalog.loaded);
        catalog.update(response(vec![local("kept", "项目", "/repo")]))?;
        for invalid in [
            json!({}),
            json!({"local-projects":{"partial":local("partial","部分","/partial"),"broken":{}}}),
            json!({"local-projects":{"wrong":local("other","错位","/one")}}),
            response(vec![
                local("one", "同目录", "/same"),
                local("two", "同目录", "/same"),
            ])?,
            json!({"local-projects":{},"project-order":{}}),
        ] {
            assert!(catalog.update(Ok(invalid)).is_err());
            assert!(catalog.loaded);
            assert_eq!(catalog.identify("/repo"), "kept");
            assert!(catalog.error.is_some());
        }
        catalog.update(response(vec![]))?;
        assert!(catalog.loaded && catalog.items.is_empty() && catalog.error.is_none());
        Ok(())
    }

    #[test]
    fn canonical_paths_match_symlinks_and_missing_directories_lexically() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("repo");
        std::fs::create_dir(&root)?;
        let alias = directory.path().join("alias");
        std::os::unix::fs::symlink(&root, &alias)?;
        let mut catalog = Catalog::default();
        catalog.update(response(vec![local(
            "linked",
            "项目",
            alias.to_str().context("目录无效")?,
        )]))?;
        assert_eq!(
            catalog.identify(root.to_str().context("目录无效")?),
            "linked"
        );
        assert_eq!(
            catalog.identify(alias.join("missing").to_str().unwrap()),
            "linked"
        );
        catalog.update(response(vec![local(
            "missing",
            "缺失目录",
            "/missing/project",
        )]))?;
        assert_eq!(catalog.identify("/missing/project/subdir"), "missing");
        Ok(())
    }

    #[test]
    fn multiple_roots_and_native_order_are_preserved_without_remote_projects() -> Result<()> {
        let mut entry = local("outer", "多根项目", "/first");
        entry["rootPaths"] = json!(["/first", "/second"]);
        let mut source = response(vec![entry, local("inner", "嵌套", "/second/nested")])?;
        source["remote-projects"] = json!({"remote":local("remote","远程","/remote")});
        let mut catalog = Catalog::default();
        catalog.update(Ok(source))?;
        assert_eq!(catalog.items[0].id, "inner");
        assert_eq!(catalog.identify("/first/task"), "outer");
        assert_eq!(catalog.identify("/second/task"), "outer");
        assert_eq!(catalog.identify("/second/nested"), "inner");
        assert!(catalog.identify("/remote").is_null());
        Ok(())
    }

    #[test]
    fn live_file_refresh_updates_snapshots_and_read_failures_preserve_classification() -> Result<()>
    {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("state.json");
        let app = crate::app::App::new(crate::config::Config {
            data_dir: directory.path().join("data"),
            sync_titles: false,
            sidebar_state_file: path.clone(),
            ..crate::config::Config::default()
        })?;
        app.store
            .discover(&[json!({"id":"chat","name":"任务","cwd":"/repo/task"})])?;
        assert!(app.snapshot()?["projectCatalog"]["loaded"] == false);
        std::fs::write(
            &path,
            response(vec![local("project", "项目", "/repo")])?.to_string(),
        )?;
        refresh(&app)?;
        assert_eq!(app.snapshot()?["sessions"][0]["projectId"], "project");
        std::fs::write(&path, "{")?;
        assert!(refresh(&app).is_err());
        let stale = app.snapshot()?;
        assert_eq!(stale["sessions"][0]["projectId"], "project");
        assert!(stale["projectCatalog"]["error"].is_string());
        std::fs::write(&path, response(vec![])?.to_string())?;
        refresh(&app)?;
        assert!(app.snapshot()?["sessions"][0]["projectId"].is_null());
        assert!(app.snapshot()?["projectCatalog"]["error"].is_null());
        Ok(())
    }

    fn worktree(root: &Path, name: &str) -> Result<PathBuf> {
        let metadata = root.join("repo/.git/worktrees").join(name);
        let checkout = root.join(name);
        std::fs::create_dir_all(&metadata)?;
        std::fs::create_dir_all(&checkout)?;
        std::fs::write(root.join("repo/.git/HEAD"), "ref: refs/heads/main\n")?;
        std::fs::write(metadata.join("HEAD"), "ref: refs/heads/topic\n")?;
        std::fs::write(metadata.join("commondir"), "../..\n")?;
        std::fs::write(
            checkout.join(".git"),
            format!("gitdir: ../repo/.git/worktrees/{name}\n"),
        )?;
        Ok(checkout)
    }

    #[test]
    fn external_worktrees_match_registered_subdirectories_and_reject_ambiguous_checkouts()
    -> Result<()> {
        let directory = tempfile::tempdir()?;
        let checkout = worktree(directory.path(), "topic")?;
        let repo = directory.path().join("repo");
        let mut catalog = Catalog::default();
        catalog.update(response(vec![local(
            "project",
            "项目",
            repo.to_str().unwrap(),
        )]))?;
        assert_eq!(catalog.assignment(checkout.to_str().unwrap())?, "project");
        let subdirectory = repo.join("sub");
        catalog.update(response(vec![local(
            "sub",
            "子项目",
            subdirectory.to_str().unwrap(),
        )]))?;
        assert!(catalog.assignment(checkout.to_str().unwrap())?.is_null());
        assert_eq!(
            catalog.assignment(checkout.join("sub/task").to_str().unwrap())?,
            "sub"
        );
        let second = worktree(directory.path(), "second")?;
        catalog.update(response(vec![
            local("one", "一", repo.to_str().unwrap()),
            local("two", "二", second.to_str().unwrap()),
        ]))?;
        assert!(catalog.assignment(checkout.to_str().unwrap()).is_err());
        let mut sessions = vec![json!({"cwd":checkout})];
        catalog.annotate(&mut sessions);
        assert!(sessions[0]["projectIssue"].is_string());
        catalog.update(response(vec![]))?;
        catalog.annotate(&mut sessions);
        assert!(sessions[0]["projectId"].is_null() && sessions[0]["projectIssue"].is_null());
        Ok(())
    }

    #[test]
    fn broken_worktree_metadata_retains_previous_membership_until_a_complete_resolution()
    -> Result<()> {
        let directory = tempfile::tempdir()?;
        let checkout = worktree(directory.path(), "topic")?;
        let mut catalog = Catalog::default();
        catalog.update(response(vec![local(
            "project",
            "项目",
            directory.path().join("repo").to_str().unwrap(),
        )]))?;
        let mut sessions = vec![json!({"cwd":checkout})];
        catalog.annotate(&mut sessions);
        std::fs::write(checkout.join(".git"), "broken")?;
        catalog.annotate(&mut sessions);
        assert_eq!(sessions[0]["projectId"], "project");
        assert!(sessions[0]["projectIssue"].is_string());
        worktree(directory.path(), "topic")?;
        catalog.annotate(&mut sessions);
        assert!(sessions[0]["projectIssue"].is_null());
        Ok(())
    }
}
