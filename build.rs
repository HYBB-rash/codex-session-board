use std::fs;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    println!("cargo:rerun-if-changed=src");
    match plugin_cannot_start_board(Path::new("src")) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("插件不得启动后台服务：{error}");
            ExitCode::from(1)
        }
    }
}

fn plugin_cannot_start_board(root: &Path) -> Result<(), String> {
    let main = fs::read_to_string(root.join("main.rs")).map_err(|error| error.to_string())?;
    let arm = mcp_arm(&main)?;
    if arm != "mcp::serve(Some(path))" {
        return Err(format!("插件入口只能调用 mcp::serve，当前是 {arm}"));
    }
    let mut files = Vec::new();
    collect(root, &mut files)?;
    for path in files {
        let name = path
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let text = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        if text.contains("managed_launch")
            || (name != "rpc/capture.rs" && text.contains("process_group(0)"))
        {
            return Err(format!("{name} 会把看板放进新的进程组"));
        }
        if text.contains(".arg(\"serve\")") || text.contains(".args([\"serve\"])") {
            return Err(format!("{name} 会再启动 serve"));
        }
        if text.contains("Command::new") && text.contains("current_exe") {
            return Err(format!("{name} 会拉起另一个看板进程"));
        }
    }
    Ok(())
}

fn mcp_arm(main: &str) -> Result<String, String> {
    let marker = "Action::Mcp =>";
    let start = main
        .find(marker)
        .ok_or_else(|| "main.rs 缺少插件入口".to_string())?;
    let rest = main[start + marker.len()..].trim_start();
    let end = rest
        .find(',')
        .ok_or_else(|| "插件入口写坏了".to_string())?;
    Ok(rest[..end].trim().to_string())
}

fn collect(dir: &Path, files: &mut Vec<std::path::PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            collect(&path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    Ok(())
}
