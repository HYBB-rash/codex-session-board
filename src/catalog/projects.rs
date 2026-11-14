use super::{
    legacy::{Projection, project_key},
    membership,
};
use serde_json::{Value, json};

pub(crate) fn catalog(server: &[Value], legacy: &Projection, host: &str) -> Vec<Value> {
    let mut projects = Vec::new();
    for project in server {
        let Some(identifier) = project["id"].as_str() else {
            continue;
        };
        let deleting = membership::deleting(identifier, legacy);
        let paths: Vec<_> = project["roots"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|root| root["path"].as_str().or_else(|| root.as_str()))
            .collect();
        projects.push(json!({"id":project_key(host,"targetServer",identifier),
            "nativeId":identifier,"hostId":host,"origin":"targetServer",
            "remote":host!="local","label":project["name"],"paths":paths,
            "pendingDeletion":deleting,
            "canCreate":false,"canMove":false,"canOpenProject":false}));
    }
    for project in &legacy.projects {
        let Some(identifier) = project["id"].as_str() else {
            continue;
        };
        let mut projection = legacy.clone();
        projection.assignments.insert(
            "catalog-project".into(),
            json!({"projectKind":"local","projectId":identifier}),
        );
        let membership =
            membership::resolve(&json!({"id":"catalog-project"}), server, &projection, host);
        let key = project_key(host, "targetLegacy", identifier);
        if membership["projectId"] != key {
            continue;
        }
        projects.push(json!({"id":key,"nativeId":identifier,"hostId":host,
            "origin":"targetLegacy","remote":host!="local","label":project["name"],
            "paths":project["rootPaths"],"canCreate":false,"canMove":false,"canOpenProject":false}));
    }
    projects
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_deletion_keeps_backend_project_visible_without_actions() {
        let mut legacy = Projection::default();
        legacy.pending_deletions.insert("old".into(), json!({"projectId":"backend"}));
        let projects = catalog(&[json!({"id":"backend","name":"Project","roots":[
            {"path":"/remote"}]})], &legacy, "remote:target");
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0]["nativeId"], "backend");
        assert_eq!(projects[0]["pendingDeletion"], true);
        for permission in ["canCreate","canMove","canOpenProject"] {
            assert_eq!(projects[0][permission], false);
        }
    }
}
