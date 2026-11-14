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
        let membership = membership::resolve(
            &json!({"id":"","projectId":identifier}),
            server,
            legacy,
            host,
        );
        if membership["state"] != "assigned" {
            continue;
        }
        let paths: Vec<_> = project["roots"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|root| root["path"].as_str().or_else(|| root.as_str()))
            .collect();
        projects.push(json!({"id":project_key(host,"targetServer",identifier),
            "nativeId":identifier,"hostId":host,"origin":"targetServer",
            "remote":host!="local","label":project["name"],"paths":paths,
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
