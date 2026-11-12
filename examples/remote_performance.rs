use anyhow::{Context, Result, ensure};

fn main() -> Result<()> {
    let mut arguments = std::env::args_os().skip(1);
    let path = arguments.next().context("需要测试配置文件路径")?;
    let host = arguments.next().context("需要登记主机 ID")?;
    ensure!(arguments.next().is_none(), "参数过多");
    let settings = codex_session_board::config::load(Some(std::path::Path::new(&path)))?;
    let report = codex_session_board::rpc::performance::run(&settings, &host.to_string_lossy())?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    ensure!(report["passed"] == true, "真实远端性能门禁未通过");
    Ok(())
}
