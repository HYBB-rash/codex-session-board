use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value, json};

const BACKING: &str = "codex.chatGptBacking";
const CREATION: &str = "codex.chatGptBackingCreation";
const RETIRED: &str = "codex.retiredChatGptBacking";

fn association(value: &Value, creation: bool) -> Result<Value> {
    ensure!(value.is_object(), "项目 ChatGPT 关联格式无效");
    let mut result = Map::new();
    for key in ["accountId", "userId"] {
        let text = value[key].as_str().filter(|text| !text.is_empty()).context("项目关联缺少所有者")?;
        result.insert(key.into(), json!(text));
    }
    if creation {
        ensure!(value["state"] == "requested", "项目 ChatGPT 创建状态无效");
        result.insert("state".into(), json!("requested"));
    } else {
        let identifier = value["projectId"].as_str().filter(|id| id.starts_with("g-p-"))
            .context("项目 ChatGPT 关联 ID 无效")?;
        result.insert("projectId".into(), json!(identifier));
    }
    Ok(Value::Object(result))
}

fn owner(value: &Value, account: &Value) -> Result<()> {
    ensure!(value["userId"] == account["userId"]
        && value["accountId"] == account["workspaceRouting"]["chatgptAccountId"],
        "项目 ChatGPT 关联属于其他账户");
    Ok(())
}

fn appearance(value: &Value, metadata: &mut Map<String, Value>) -> Result<()> {
    if value.is_null() {
        return Ok(());
    }
    let color = value["color"].as_str().context("项目外观缺少颜色")?;
    ensure!(["black","blue","green","orange","pink","purple","red","yellow"].contains(&color),
        "项目颜色无效");
    let marker = &value["marker"];
    let normalized = match marker["kind"].as_str() {
        Some("emoji") => json!({"kind":"emoji","emoji":marker["emoji"].as_str()
            .filter(|text| !text.is_empty()).context("项目表情无效")?}),
        Some("icon") => {
            let icon = marker["icon"].as_str().context("项目图标无效")?;
            let icon = match icon {
                "balancing-scale"=>"scale", "cat"=>"paw", "code"|"code-brackets"=>"function",
                "globe-spin"=>"desk-globe", "graduation"=>"graduation-cap",
                "lightbulb"|"network"=>"brain", "lite"=>"lotus", "notebook"=>"logs",
                "pencil"=>"edit", "pens"|"puzzle"=>"customize", "presentation"=>"bar-chart",
                "search"=>"globe", "waveform"=>"music",
                "building"|"bug"|"cube"|"gift"|"lightning"|"openai"|"pointer"|"star"|"target"=>"folder",
                canonical=>canonical,
            };
            ensure!(["folder","currency-dollar","book","graduation-cap","edit","writing","function",
                "terminal","music","popcorn","customize","palette","stethoscope","health","lotus",
                "suitcase","bar-chart","kettlebell","dumbbell","logs","scale","desk-globe","plane",
                "globe","wrench","paw","flask","brain","heart","plant"].contains(&icon), "项目图标无效");
            json!({"kind":"icon","icon":icon})
        }
        _ => anyhow::bail!("项目标记无效"),
    };
    metadata.insert("appearance.color".into(), json!(color));
    metadata.insert("appearance.marker".into(), json!(serde_json::to_string(&normalized)?));
    Ok(())
}

pub(super) fn convert(identifier: &str, project: &Value, style: &Value, account: &Value) -> Result<Value> {
    let mut metadata = Map::new();
    appearance(style, &mut metadata)?;
    let retired = &project["retiredChatGptBacking"];
    if !retired.is_null() {
        let retired = association(retired, false)?;
        if !project["chatGptBacking"].is_null() {
            ensure!(association(&project["chatGptBacking"], false)? == retired,
                "项目退休关联与当前关联冲突");
        }
        metadata.insert(RETIRED.into(), json!(serde_json::to_string(&retired)?));
        return Ok(Value::Object(metadata));
    }
    for (field, key, creation) in [
        ("chatGptBacking", BACKING, false), ("chatGptBackingCreation", CREATION, true),
    ] {
        if !project[field].is_null() {
            let backing = association(&project[field], creation)?;
            owner(&backing, account)?;
            metadata.insert(key.into(), json!(serde_json::to_string(&backing)?));
            if !creation {
                metadata.insert("codex.localProjectId".into(), json!(identifier));
            }
        }
    }
    Ok(Value::Object(metadata))
}

pub(super) fn validate_canonical(metadata: &Value, account: &Value, legacy: Option<&str>) -> Result<()> {
    ensure!(metadata.is_object(), "规范项目缺少 metadata");
    if let Some(alias) = metadata.get("codex.localProjectId") {
        ensure!(legacy.is_none_or(|identifier| alias == identifier), "规范项目指向其他旧项目");
    }
    let retired = metadata.get(RETIRED).map(|value| -> Result<Value> {
        association(&serde_json::from_str::<Value>(value.as_str().context("退休关联格式无效")?)?, false)
    }).transpose()?;
    for (key, creation) in [(BACKING, false), (CREATION, true)] {
        if let Some(value) = metadata.get(key) {
            ensure!(retired.is_none(), "规范项目同时包含退休及活动关联");
            let backing = association(&serde_json::from_str::<Value>(value.as_str()
                .context("规范项目关联格式无效")?)?, creation)?;
            owner(&backing, account)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account() -> Value {
        json!({"userId":"user","workspaceRouting":{"chatgptAccountId":"account"}})
    }

    fn backing() -> Value {
        json!({"userId":"user","accountId":"account","projectId":"g-p-test"})
    }

    #[test]
    fn native_metadata_preserves_appearance_backing_creation_and_retirement() -> Result<()> {
        let style = json!({"color":"blue","marker":{"kind":"icon","icon":"cat"}});
        let metadata = convert("legacy", &json!({"chatGptBacking":backing(),
            "chatGptBackingCreation":{"userId":"user","accountId":"account","state":"requested"}}),
            &style, &account())?;
        assert_eq!(metadata["appearance.color"], "blue");
        assert_eq!(serde_json::from_str::<Value>(metadata["appearance.marker"].as_str().context("marker")?)?,
            json!({"kind":"icon","icon":"paw"}));
        assert_eq!(metadata["codex.localProjectId"], "legacy");
        validate_canonical(&metadata, &account(), Some("legacy"))?;
        let retired = convert("legacy",&json!({"retiredChatGptBacking":backing()}),
            &Value::Null,&account())?;
        assert_eq!(retired.as_object().context("metadata")?.len(), 1);
        validate_canonical(&retired, &account(), Some("legacy"))?;
        Ok(())
    }

    #[test]
    fn invalid_owner_backing_and_appearance_fail_before_import() {
        let mut wrong = backing();
        wrong["userId"] = json!("other");
        assert!(convert("legacy",&json!({"chatGptBacking":wrong}),&Value::Null,&account()).is_err());
        assert!(convert("legacy",&json!({"chatGptBackingCreation":
            {"userId":"other","accountId":"account","state":"requested"}}),&Value::Null,&account()).is_err());
        assert!(convert("legacy",&json!({"retiredChatGptBacking":backing(),"chatGptBacking":wrong}),
            &Value::Null,&account()).is_err());
        assert!(convert("legacy",&json!({}),&json!({"color":"blue","marker":{"kind":"icon","icon":"bad"}}),
            &account()).is_err());
    }
}
