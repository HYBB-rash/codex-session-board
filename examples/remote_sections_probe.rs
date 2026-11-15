use anyhow::{Result, ensure};
use codex_session_board::{config, rpc::section_probe};
use std::path::Path;

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(args.len() == 4 || args.len() == 6 && args[4] == "--cleanup",
        "需要配置文件、主机 ID、私有实验目录、已核验的客户端分区端点；可附加 --cleanup 已记录的专用空会话 ID");
    let settings = config::load(Some(Path::new(&args[0])))?;
    let cleanup = args.get(5).map(|value| value.to_string_lossy());
    let report = section_probe::run(&settings,&args[1].to_string_lossy(),
        Path::new(&args[2]),Path::new(&args[3]),cleanup.as_deref())?;
    println!("{}",serde_json::to_string_pretty(&report)?);
    Ok(())
}
