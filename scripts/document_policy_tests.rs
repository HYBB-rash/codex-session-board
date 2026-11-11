use super::*;
use std::fs;
use tempfile::tempdir;

fn write_files(root: &Path, names: &[&str]) -> Result<()> {
    for name in names {
        fs::write(root.join(name), b"fixture")?;
    }
    Ok(())
}

fn expected_paths(names: &[&str]) -> Vec<PathBuf> {
    let mut paths = names.iter().map(PathBuf::from).collect::<Vec<_>>();
    paths.sort();
    paths
}

#[test]
fn exact_root_document_names_are_allowed() -> Result<()> {
    let directory = tempdir()?;
    write_files(directory.path(), &["AGENTS.md", "README.md"])?;
    assert!(violations(directory.path())?.is_empty());
    assert!(check(directory.path())?);
    Ok(())
}

#[test]
fn all_specified_document_extensions_are_reported() -> Result<()> {
    let directory = tempdir()?;
    let names = [
        "guide.md",
        "guide.markdown",
        "guide.mdown",
        "guide.mkd",
        "guide.mdx",
        "guide.rst",
        "guide.rest",
        "guide.adoc",
        "guide.asciidoc",
        "guide.org",
        "guide.rdoc",
        "guide.txt",
        "guide.text",
        "guide.pdf",
        "guide.doc",
        "guide.docx",
        "guide.odt",
        "guide.rtf",
        "guide.tex",
    ];
    write_files(directory.path(), &names)?;
    assert_eq!(violations(directory.path())?, expected_paths(&names));
    assert!(!check(directory.path())?);
    Ok(())
}

#[test]
fn extensions_ignore_case_but_root_allowance_requires_exact_case() -> Result<()> {
    let directory = tempdir()?;
    write_files(directory.path(), &["AGENTS.md", "README.md"])?;
    for extension in DOCUMENT_EXTENSIONS {
        fs::write(
            directory
                .path()
                .join(format!("guide.{}", extension.to_ascii_uppercase())),
            [],
        )?;
    }
    let variants = [
        "agents.md",
        "AGENTS.MD",
        "readme.md",
        "README.MD",
        "Readme.md",
    ];
    let mut found = violations(directory.path())?;
    for name in variants {
        let sample = tempdir()?;
        write_files(sample.path(), &[name])?;
        let reported = violations(sample.path())?;
        assert_eq!(reported, [PathBuf::from(name)]);
        found.extend(reported);
    }
    assert_eq!(found.len(), DOCUMENT_EXTENSIONS.len() + variants.len());
    for name in variants {
        assert!(found.contains(&PathBuf::from(name)));
    }
    assert!(!found.contains(&PathBuf::from("AGENTS.md")));
    assert!(!found.contains(&PathBuf::from("README.md")));
    Ok(())
}

#[test]
fn common_extensionless_documents_ignore_case() -> Result<()> {
    let directory = tempdir()?;
    let names = [
        "readme",
        "Changelog",
        "CHANGES",
        "history",
        "CONTRIBUTING",
        "Install",
        "usage",
        "DESIGN",
        "architecture",
        "todo",
        "FAQ",
        "Notes",
    ];
    write_files(directory.path(), &names)?;
    assert_eq!(violations(directory.path())?, expected_paths(&names));
    Ok(())
}

#[test]
fn program_input_txt_exceptions_support_named_variants_and_case() -> Result<()> {
    let directory = tempdir()?;
    let names = [
        "CMakeLists.txt",
        "CMAKELISTS.TXT",
        "requirements.txt",
        "Requirements.TXT",
        "constraints.txt",
        "CONSTRAINTS.TXT",
        "meson_options.txt",
        "MESON_OPTIONS.TXT",
        "requirements-dev.txt",
        "requirements_test.TXT",
        "REQUIREMENTS-PROD.TXT",
        "constraints-prod.txt",
        "constraints_ci.txt",
        "CONSTRAINTS_CI.TXT",
    ];
    write_files(directory.path(), &names)?;
    assert!(check(directory.path())?);
    let other = [
        "other_constraints.txt",
        "guide.txt",
        "requirements-dev.md",
        "constraints_ci.markdown",
        "requirements-dev.text",
        "myrequirements-dev.txt",
        "requirementsdev.txt",
        "constraintsci.txt",
    ];
    write_files(directory.path(), &other)?;
    assert_eq!(violations(directory.path())?, expected_paths(&other));
    Ok(())
}

#[test]
fn cargo_configuration_data_sources_scripts_and_binaries_are_not_documents() -> Result<()> {
    let directory = tempdir()?;
    let names = [
        "Cargo.toml",
        "Cargo.lock",
        "config.json",
        "events.jsonl",
        "config.yaml",
        "config.yml",
        "table.csv",
        "page.html",
        "README.json",
        "main.rs",
        "worker.py",
        "check.sh",
        "check",
        "app.exe",
        "library.so",
        "LICENSE",
        "NOTICE",
        "AGENTS",
        ".gitignore",
        ".env",
    ];
    write_files(directory.path(), &names)?;
    fs::write(directory.path().join("app.exe"), [0, 0xff, 1])?;
    assert!(check(directory.path())?);
    Ok(())
}

#[test]
fn license_and_notice_are_not_exempt_from_document_extensions() -> Result<()> {
    let directory = tempdir()?;
    let names = ["LICENSE.md", "NOTICE.txt", "README.txt"];
    write_files(directory.path(), &names)?;
    assert_eq!(violations(directory.path())?, expected_paths(&names));
    Ok(())
}

#[test]
fn scanning_ignores_nested_documents_and_directories_named_like_documents() -> Result<()> {
    let directory = tempdir()?;
    let root = directory.path();
    for name in [
        "Docs",
        "nested",
        "folder.md",
        "notes.txt",
        "README",
        "README.md",
    ] {
        fs::create_dir(root.join(name))?;
        fs::write(root.join(name).join("inside.md"), b"nested document")?;
    }
    assert!(check(root)?);
    Ok(())
}

#[test]
fn new_files_are_checked_without_a_git_repository_and_sorted_stably() -> Result<()> {
    let directory = tempdir()?;
    let root = directory.path();
    assert!(check(root)?);
    let names = ["z.md", "a.pdf", "规划 草稿.md", "中间\n换行.txt", "B.rst"];
    write_files(root, &names)?;
    assert!(!root.join(".git").exists());
    let found = violations(root)?;
    assert_eq!(found, expected_paths(&names));
    assert_eq!(violations(root)?, found);
    assert!(!check(root)?);
    Ok(())
}

#[test]
fn reports_name_and_docs_destination_without_changing_contents() -> Result<()> {
    let directory = tempdir()?;
    let root = directory.path();
    let content = [0xff, 0, b'/', b'*', b'\n'];
    fs::write(root.join("report.pdf"), content)?;
    fs::write(root.join("README.md"), b"keep the introduction")?;
    let before = fs::metadata(root.join("report.pdf"))?.modified()?;
    assert!(!check(root)?);
    assert_eq!(fs::read(root.join("report.pdf"))?, content);
    assert_eq!(fs::read(root.join("README.md"))?, b"keep the introduction");
    assert_eq!(fs::metadata(root.join("report.pdf"))?.modified()?, before);
    assert!(!root.join("Docs").exists());
    let message = violation_message(Path::new("report.pdf"));
    assert!(message.contains("report.pdf"));
    assert!(message.contains("移至 Docs/"));
    assert!(message.contains("Docs/已存在同名文件时先比较内容，不直接覆盖"));
    Ok(())
}

#[test]
fn invalid_roots_return_errors_instead_of_success() -> Result<()> {
    let directory = tempdir()?;
    assert!(check(&directory.path().join("missing")).is_err());
    fs::write(directory.path().join("file"), b"not a directory")?;
    assert!(check(&directory.path().join("file")).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn document_file_links_are_reported_but_directory_links_are_ignored() -> Result<()> {
    use std::os::unix::fs::symlink;
    let directory = tempdir()?;
    let external = tempdir()?;
    let root = directory.path();
    fs::create_dir(root.join("Docs"))?;
    fs::write(external.path().join("source"), b"external data")?;
    symlink(external.path().join("source"), root.join("guide.md"))?;
    symlink(external.path().join("source"), root.join("README.md"))?;
    symlink(root.join("Docs"), root.join("directory.md"))?;
    symlink(root.join("missing"), root.join("dangling.txt"))?;
    let expected = expected_paths(&["guide.md", "dangling.txt"]);
    assert_eq!(violations(root)?, expected);
    assert!(!check(root)?);
    assert_eq!(fs::read(external.path().join("source"))?, b"external data");
    assert!(fs::symlink_metadata(root.join("guide.md"))?.is_symlink());
    assert!(fs::symlink_metadata(root.join("dangling.txt"))?.is_symlink());
    Ok(())
}

#[cfg(unix)]
#[test]
fn non_utf8_document_filenames_keep_their_original_bytes() -> Result<()> {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let name = OsString::from_vec(vec![0xff, b'.', b'M', b'D']);
    assert_eq!(document_entry(&name, || Ok(true))?, Some(PathBuf::from(&name)));
    #[cfg(target_os = "linux")]
    {
        let directory = tempdir()?;
        fs::write(directory.path().join(&name), b"document")?;
        assert_eq!(violations(directory.path())?, [PathBuf::from(name)]);
    }
    Ok(())
}
