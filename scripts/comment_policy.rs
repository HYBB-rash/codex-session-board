use anyhow::{Context, Result, bail, ensure};
use ra_ap_rustc_lexer::{FrontmatterAllowed, LiteralKind, TokenKind, strip_shebang, tokenize};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

type Baseline = BTreeMap<PathBuf, BTreeMap<String, usize>>;
type Current = BTreeMap<PathBuf, Vec<Comment>>;

#[derive(Debug, PartialEq, Eq)]
struct Comment {
    text: String,
    line: usize,
}

#[derive(Default)]
struct Counts {
    existing: usize,
    added: usize,
}

pub fn check(root: &Path, repository: &Path, baseline: &str) -> Result<bool> {
    ensure!(root.is_dir(), "注释检查根目录不存在或不是目录：{root:?}");
    let commit = validate_baseline(repository, baseline)?;
    let mut known = baseline_comments(repository, &commit)?;
    let current = current_comments(root, repository)?;
    let counts = report_comments(&mut known, current);
    let status = if counts.added == 0 {
        "通过"
    } else {
        "未通过"
    };
    println!(
        "[{status}] 普通 Rust 注释：基线 {commit}；当前保留 {} 处，新增 {} 处。",
        counts.existing, counts.added
    );
    if counts.added > 0 {
        println!(
            "处理建议：先改善命名、类型或代码结构，让代码表达用途，再运行测试验证行为；不要为通过检查机械删除注释。"
        );
    }
    Ok(counts.added == 0)
}

fn git(repository: &Path, arguments: &[&OsStr]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .output()
        .with_context(|| format!("无法在仓库 {:?} 运行 Git", repository))?;
    ensure!(
        output.status.success(),
        "Git 注释核对失败（状态 {:?}，参数 {:?}）：{}",
        output.status.code(),
        arguments,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(output.stdout)
}

fn validate_baseline(repository: &Path, baseline: &str) -> Result<String> {
    ensure!(
        matches!(baseline.len(), 40 | 64) && baseline.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "注释基线必须是完整、固定的 Git 提交 ID，不能使用 HEAD、标签或缩写"
    );
    let object = format!("{baseline}^{{commit}}");
    let resolved = git(
        repository,
        &[
            OsStr::new("rev-parse"),
            OsStr::new("--verify"),
            OsStr::new(&object),
        ],
    )
    .context("注释基线提交不存在或不是提交对象")?;
    let commit = std::str::from_utf8(&resolved)?.trim();
    ensure!(
        commit.eq_ignore_ascii_case(baseline),
        "注释基线必须直接指定完整提交 ID，不能指定其他对象"
    );
    git(
        repository,
        &[
            OsStr::new("merge-base"),
            OsStr::new("--is-ancestor"),
            OsStr::new(commit),
            OsStr::new("HEAD"),
        ],
    )
    .context("注释基线不是当前仓库 HEAD 的祖先，无法完成核对")?;
    Ok(commit.to_string())
}

fn rust_paths(output: &[u8]) -> Result<BTreeSet<PathBuf>> {
    output
        .split(|byte| *byte == 0)
        .filter(|path| path.ends_with(b".rs"))
        .map(|bytes| {
            let path = path_from_bytes(bytes)?;
            ensure!(
                path.components()
                    .all(|component| matches!(component, Component::Normal(_))),
                "Git 返回了不安全的相对路径：{path:?}"
            );
            Ok(path)
        })
        .collect()
}

#[cfg(unix)]
fn path_from_bytes(bytes: &[u8]) -> Result<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    Ok(PathBuf::from(OsString::from_vec(bytes.to_vec())))
}

#[cfg(not(unix))]
fn path_from_bytes(bytes: &[u8]) -> Result<PathBuf> {
    Ok(PathBuf::from(
        std::str::from_utf8(bytes).context("Rust 文件路径不是有效 UTF-8")?,
    ))
}

fn baseline_comments(repository: &Path, commit: &str) -> Result<Baseline> {
    let output = git(
        repository,
        &[
            OsStr::new("ls-tree"),
            OsStr::new("-r"),
            OsStr::new("--name-only"),
            OsStr::new("-z"),
            OsStr::new(commit),
        ],
    )?;
    let mut baseline = BTreeMap::new();
    for path in rust_paths(&output)? {
        let mut object = OsString::from(commit);
        object.push(":");
        object.push(&path);
        let bytes = git(
            repository,
            &[OsStr::new("cat-file"), OsStr::new("blob"), &object],
        )?;
        let source = String::from_utf8(bytes)
            .with_context(|| format!("基线 Rust 文件不是有效 UTF-8：{path:?}"))?;
        let mut counts = BTreeMap::new();
        for comment in
            comments(&source).with_context(|| format!("无法检查基线 Rust 文件：{path:?}"))?
        {
            *counts.entry(comment.text).or_default() += 1;
        }
        baseline.insert(path, counts);
    }
    Ok(baseline)
}

fn current_comments(root: &Path, repository: &Path) -> Result<Current> {
    let output = git(
        repository,
        &[
            OsStr::new("ls-files"),
            OsStr::new("--cached"),
            OsStr::new("--others"),
            OsStr::new("--exclude-standard"),
            OsStr::new("-z"),
        ],
    )?;
    let mut current = BTreeMap::new();
    for path in rust_paths(&output)? {
        let candidate = root.join(&path);
        if !candidate
            .try_exists()
            .with_context(|| format!("无法检查 Rust 文件是否存在：{path:?}"))?
        {
            continue;
        }
        let source = fs::read_to_string(&candidate)
            .with_context(|| format!("无法读取待检查 Rust 文件：{path:?}"))?;
        let found =
            comments(&source).with_context(|| format!("无法检查当前 Rust 文件：{path:?}"))?;
        current.insert(path, found);
    }
    Ok(current)
}

fn report_comments(baseline: &mut Baseline, current: Current) -> Counts {
    let mut counts = Counts::default();
    for (path, comments) in current {
        let known = baseline.entry(path.clone()).or_default();
        for comment in comments {
            let remaining = known.entry(comment.text).or_default();
            if *remaining > 0 {
                *remaining -= 1;
                counts.existing += 1;
                println!(
                    "[已有] {:?}:{}：基线已有普通注释，可保留；是否整理由用户决定。",
                    path, comment.line
                );
            } else {
                counts.added += 1;
                println!(
                    "[新增] {:?}:{}：新增或修改了普通 Rust 注释，需先改善命名、类型或结构并测试。",
                    path, comment.line
                );
            }
        }
    }
    counts
}

fn comments(source: &str) -> Result<Vec<Comment>> {
    let body = source.strip_prefix('\u{feff}').unwrap_or(source);
    let body = &body[strip_shebang(body).unwrap_or(0)..];
    let prefix = source.len() - body.len();
    let mut line = 1 + source[..prefix]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count();
    let mut offset = 0;
    let mut comments = Vec::new();
    for token in tokenize(body, FrontmatterAllowed::No) {
        let end = offset + token.len as usize;
        let text = body.get(offset..end).context("词法器返回了无效字符边界")?;
        if let Some(problem) = token_error(token.kind) {
            bail!("第 {line} 行：{problem}；请先修复词法错误，再运行注释检查");
        }
        if matches!(
            token.kind,
            TokenKind::LineComment { doc_style: None }
                | TokenKind::BlockComment {
                    doc_style: None,
                    ..
                }
        ) {
            comments.push(Comment {
                text: text.to_string(),
                line,
            });
        }
        line += text.bytes().filter(|byte| *byte == b'\n').count();
        offset = end;
    }
    Ok(comments)
}

fn token_error(kind: TokenKind) -> Option<&'static str> {
    match kind {
        TokenKind::BlockComment {
            terminated: false, ..
        } => Some("块注释未闭合"),
        TokenKind::Literal { kind, .. } => literal_error(kind),
        TokenKind::Unknown | TokenKind::InvalidIdent => Some("无法识别的 Rust 词法单元"),
        TokenKind::UnknownPrefix
        | TokenKind::UnknownPrefixLifetime
        | TokenKind::GuardedStrPrefix => Some("无效或保留的 Rust 词法前缀"),
        TokenKind::Lifetime {
            starts_with_number: true,
        } => Some("生命周期标识符无效"),
        _ => None,
    }
}

fn literal_error(kind: LiteralKind) -> Option<&'static str> {
    match kind {
        LiteralKind::Char { terminated: false }
        | LiteralKind::Byte { terminated: false }
        | LiteralKind::Str { terminated: false }
        | LiteralKind::ByteStr { terminated: false }
        | LiteralKind::CStr { terminated: false } => Some("字符或字符串字面量未闭合"),
        LiteralKind::RawStr { n_hashes: None }
        | LiteralKind::RawByteStr { n_hashes: None }
        | LiteralKind::RawCStr { n_hashes: None } => Some("原始字符串字面量无效或未闭合"),
        LiteralKind::Int {
            empty_int: true, ..
        }
        | LiteralKind::Float {
            empty_exponent: true,
            ..
        } => Some("数字字面量不完整"),
        _ => None,
    }
}

#[cfg(test)]
#[path = "comment_policy_tests.rs"]
mod tests;
