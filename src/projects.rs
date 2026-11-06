use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

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

    pub fn annotate(&self, sessions: &mut [Value]) {
        let mut paths = HashMap::new();
        for session in sessions {
            let cwd = session["cwd"].as_str().unwrap_or("").to_owned();
            session["projectId"] = paths
                .entry(cwd.clone())
                .or_insert_with(|| self.identify(&cwd))
                .clone();
        }
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
}
