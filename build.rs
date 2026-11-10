use std::fs;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    println!("cargo:rerun-if-changed=src");
    match plugin_stays_in_process(Path::new("src")) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("插件不得启动后台服务：{error}");
            ExitCode::from(1)
        }
    }
}

fn plugin_stays_in_process(root: &Path) -> Result<(), String> {
    let mut files = Vec::new();
    collect(root, &mut files)?;
    for path in files {
        let text = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let name = path
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if name != "main.rs" && name != "service.rs" && text.contains("service::launch") {
            return Err(format!("{name} 调用了 service::launch"));
        }
        if name != "service.rs" && text.contains(".arg(\"serve\")") {
            return Err(format!("{name} 会再启动 serve"));
        }
        if name != "rpc/capture.rs" && name != "service.rs" && text.contains("process_group(0)") {
            return Err(format!("{name} 把进程放进了新的进程组"));
        }
    }
    Ok(())
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
