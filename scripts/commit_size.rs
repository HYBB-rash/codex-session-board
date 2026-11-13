use super::{git, git_output};
use anyhow::{Context, Result, ensure};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

#[derive(Debug, Default, PartialEq, Eq)]
struct Statistics {
    files: usize,
    added: usize,
    deleted: usize,
    binaries: Vec<Vec<u8>>,
}

impl Statistics {
    fn allowed(&self) -> bool {
        self.files <= 13 && self.added + self.deleted <= 400
    }

    fn record(&mut self, added: &[u8], deleted: &[u8], path: &[u8]) -> Result<()> {
        self.files += 1;
        if added == b"-" && deleted == b"-" {
            self.binaries.push(path.to_vec());
        } else {
            self.added = self
                .added
                .checked_add(number(added)?)
                .context("新增行数溢出")?;
            self.deleted = self
                .deleted
                .checked_add(number(deleted)?)
                .context("删除行数溢出")?;
            self.added
                .checked_add(self.deleted)
                .context("总变更行数溢出")?;
        }
        Ok(())
    }
}

fn number(value: &[u8]) -> Result<usize> {
    let text = std::str::from_utf8(value)?;
    ensure!(
        !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()),
        "无效的 Git 行数"
    );
    Ok(text.parse()?)
}

fn statistics(output: &[u8]) -> Result<Statistics> {
    ensure!(
        output.is_empty() || output.ends_with(&[0]),
        "Git 统计记录未完整终止"
    );
    let mut records = output.split(|byte| *byte == 0);
    let mut total = Statistics::default();
    while let Some(record) = records.next() {
        if record.is_empty() {
            ensure!(records.next().is_none(), "Git 统计包含意外的空记录");
            break;
        }
        let mut fields = record.splitn(3, |byte| *byte == b'\t');
        let added = fields.next().context("缺少新增行数")?;
        let deleted = fields.next().context("缺少删除行数")?;
        let path = fields.next().context("缺少文件路径")?;
        let path = if path.is_empty() {
            let old = records.next().context("重命名缺少原路径")?;
            ensure!(!old.is_empty(), "重命名原路径为空");
            records.next().context("重命名缺少目标路径")?
        } else {
            path
        };
        ensure!(!path.is_empty(), "Git 文件路径为空");
        total.record(added, deleted, path)?;
    }
    Ok(total)
}

fn measure(repository: &Path, snapshot: &Path) -> Result<Statistics> {
    measure_against(repository, snapshot, None)
}

fn measure_against(repository: &Path, snapshot: &Path, base: Option<&str>) -> Result<Statistics> {
    let git_directory = git(repository, &["rev-parse", "--absolute-git-dir"])?;
    let snapshot = snapshot.to_str().context("暂存快照路径不是 UTF-8")?;
    let mut arguments = vec![
        "--git-dir",
        git_directory.trim_end_matches('\n'),
        "--work-tree",
        snapshot,
        "-C",
        snapshot,
        "-c",
        "core.attributesFile=/dev/null",
        "diff",
        "--cached",
        "--numstat",
        "-z",
        "--no-ext-diff",
        "--no-textconv",
        "--no-renames",
        "--find-renames=50%",
        "--ignore-submodules=none",
        "--ita-invisible-in-index",
    ];
    if let Some(base) = base {
        arguments.push(base);
    }
    arguments.push("--");
    let output = git_output(repository, &arguments)?;
    ensure!(
        output.status.success(),
        "无法统计暂存区：{}",
        String::from_utf8_lossy(&output.stderr)
    );
    statistics(&output.stdout)
}

pub fn check(repository: &Path, snapshot: &Path) -> Result<bool> {
    let total = match merge_size::measure(repository, snapshot)? {
        Some(total) => total,
        None => measure(repository, snapshot)?,
    };
    let passed = total.allowed();
    let label = if passed { "通过" } else { "未通过" };
    println!(
        "[{label}] 暂存区提交规模：{} 个文件，新增 {} 行，删除 {} 行，总变更 {} 行（上限 13 个文件、400 行）。",
        total.files,
        total.added,
        total.deleted,
        total.added + total.deleted
    );
    for path in &total.binaries {
        println!(
            "  二进制文件 {:?}：已计入文件数，无法按文本行数衡量。",
            Path::new(OsStr::from_bytes(path))
        );
    }
    if total.files > 13 {
        println!("  文件数超过 13 个。");
    }
    if total.added + total.deleted > 400 {
        println!("  总变更行数超过 400 行。");
    }
    if !passed {
        println!("  请按独立、可验证的步骤拆分提交；保留现有改动，不通过删除或忽略文件凑上限。");
    }
    Ok(passed)
}

#[path = "merge_size.rs"]
mod merge_size;

#[cfg(test)]
#[path = "commit_size_integration_tests.rs"]
mod integration_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_limits_and_binary_rename_are_counted() -> Result<()> {
        let mut output = b"200\t200\tfirst\0-\t-\t\0old\0new\0".to_vec();
        output.extend(b"0\t0\tfile\0".repeat(11));
        let total = statistics(&output)?;
        assert_eq!((total.files, total.added, total.deleted), (13, 200, 200));
        assert_eq!(total.binaries, vec![b"new".to_vec()]);
        assert!(total.allowed());
        assert!(!statistics(&[output.clone(), b"0\t0\textra\0".to_vec()].concat())?.allowed());
        assert!(!statistics(b"201\t200\tfile\0")?.allowed());
        Ok(())
    }

    #[test]
    fn malformed_statistics_fail_closed_and_binary_paths_remain_bytes() -> Result<()> {
        let malformed: &[&[u8]] = &[
            b"1\t0\tfile",
            b"x\t0\tfile\0",
            b"-\t1\tfile\0",
            b"1\t-\tfile\0",
            b"0\t0\t\0old\0",
            b"0\t0\t\0\0new\0",
            b"0\t0\tfile\0\0extra\0",
        ];
        for record in malformed {
            assert!(statistics(record).is_err(), "{record:?}");
        }
        assert!(statistics(format!("{}\t1\tfile\0", usize::MAX).as_bytes()).is_err());
        let total = statistics(b"-\t-\t-\xff\tname\n\0")?;
        assert_eq!(total.binaries, vec![b"-\xff\tname\n".to_vec()]);
        assert!(statistics(b"")?.allowed());
        Ok(())
    }
}
