use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::fs::{self, DirEntry};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

pub const ROOT_DOCUMENTS: &[&str] = &["AGENTS.md", "README.md"];
pub const DOCUMENT_EXTENSIONS: &[&str] = &[
    "md", "markdown", "mdown", "mkd", "mdx", "rst", "rest", "adoc", "asciidoc", "org", "rdoc",
    "txt", "text", "pdf", "doc", "docx", "odt", "rtf", "tex",
];
pub const PROGRAM_INPUT_FILES: &[&str] = &[
    "CMakeLists.txt",
    "requirements.txt",
    "constraints.txt",
    "meson_options.txt",
];
pub const PROGRAM_INPUT_STEM_PREFIXES: &[&str] = &[
    "requirements-",
    "requirements_",
    "constraints-",
    "constraints_",
];
pub const EXTENSIONLESS_DOCUMENTS: &[&str] = &[
    "README",
    "CHANGELOG",
    "CHANGES",
    "HISTORY",
    "CONTRIBUTING",
    "INSTALL",
    "USAGE",
    "DESIGN",
    "ARCHITECTURE",
    "TODO",
    "FAQ",
    "NOTES",
];

pub fn check(root: &Path) -> Result<bool> {
    let documents = violations(root)?;
    if documents.is_empty() {
        println!("[通过] 根目录文档检查：仅允许 AGENTS.md 和 README.md，未发现违规文档。");
        return Ok(true);
    }
    for document in &documents {
        println!("{}", violation_message(document));
    }
    println!(
        "[未通过] 根目录共有 {} 份文档需要整理；检查不会自动移动或删除文件。",
        documents.len()
    );
    Ok(false)
}

pub fn violations(root: &Path) -> Result<Vec<PathBuf>> {
    let entries = fs::read_dir(root).with_context(|| format!("无法读取待检查根目录：{root:?}"))?;
    let mut documents = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| format!("无法读取根目录中的条目：{root:?}"))?;
        if is_document_name(&entry.file_name()) && is_file_entry(&entry)? {
            documents.push(PathBuf::from(entry.file_name()));
        }
    }
    documents.sort();
    Ok(documents)
}

fn is_document_name(name: &OsStr) -> bool {
    if let Some(name) = name.to_str() {
        if ROOT_DOCUMENTS.contains(&name) || is_program_input(name) {
            return false;
        }
        if EXTENSIONLESS_DOCUMENTS
            .iter()
            .any(|document| document.eq_ignore_ascii_case(name))
        {
            return true;
        }
    }
    Path::new(name)
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| {
            DOCUMENT_EXTENSIONS
                .iter()
                .any(|document| document.eq_ignore_ascii_case(extension))
        })
}

fn is_program_input(name: &str) -> bool {
    if PROGRAM_INPUT_FILES
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(name))
    {
        return true;
    }
    let lowercase = name.to_ascii_lowercase();
    lowercase.strip_suffix(".txt").is_some_and(|stem| {
        PROGRAM_INPUT_STEM_PREFIXES
            .iter()
            .any(|prefix| stem.starts_with(prefix))
    })
}

fn is_file_entry(entry: &DirEntry) -> Result<bool> {
    let kind = entry
        .file_type()
        .with_context(|| format!("无法读取根目录条目类型：{:?}", entry.path()))?;
    if !kind.is_symlink() {
        return Ok(kind.is_file());
    }
    match fs::metadata(entry.path()) {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error)
            .with_context(|| format!("无法确认根目录文档符号链接类型：{:?}", entry.path())),
    }
}

fn violation_message(document: &Path) -> String {
    format!(
        "[未通过] 根目录文档 {document:?} 不符合约定；请移至 Docs/，并更新相关引用。Docs/已存在同名文件时先比较内容，不直接覆盖。"
    )
}

#[cfg(test)]
#[path = "document_policy_tests.rs"]
mod tests;
