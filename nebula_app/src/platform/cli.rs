//! Native CLI package layouts and launcher conventions.
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn codex_executable(launcher: PathBuf) -> Option<PathBuf> {
    let launcher = std::fs::canonicalize(&launcher).unwrap_or(launcher);
    if launcher
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
    {
        return Some(launcher);
    }
    let triple = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "windows") => "x86_64-pc-windows-msvc",
        ("aarch64", "windows") => "aarch64-pc-windows-msvc",
        ("x86_64", "linux") => "x86_64-unknown-linux-musl",
        ("aarch64", "linux") => "aarch64-unknown-linux-musl",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        ("aarch64", "macos") => "aarch64-apple-darwin",
        _ => return None,
    };
    let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "win32-x64",
        ("windows", "aarch64") => "win32-arm64",
        ("linux", "x86_64") => "linux-x64",
        ("linux", "aarch64") => "linux-arm64",
        ("macos", "x86_64") => "darwin-x64",
        ("macos", "aarch64") => "darwin-arm64",
        _ => return None,
    };
    let binary = if cfg!(windows) { "codex.exe" } else { "codex" };
    for parent in launcher.ancestors().skip(1).take(3) {
        for package in [
            parent.to_path_buf(),
            parent.join("node_modules/@openai/codex"),
            parent.join(format!("node_modules/@openai/codex-{platform}")),
            parent
                .join(format!("node_modules/@openai/codex/node_modules/@openai/codex-{platform}")),
            parent.join(format!("codex-{platform}")),
        ] {
            for directory in ["bin", "codex"] {
                let candidate = package.join("vendor").join(triple).join(directory).join(binary);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    #[cfg(unix)]
    {
        use std::io::Read as _;
        let mut header = [0; 4];
        if std::fs::File::open(&launcher).and_then(|mut file| file.read_exact(&mut header)).is_ok()
            && (header == *b"\x7fELF"
                || matches!(header, [0xcf, 0xfa, 0xed, 0xfe] | [0xca, 0xfe, 0xba, 0xbe]))
        {
            return Some(launcher);
        }
    }
    None
}

pub(crate) fn npm_prefix(package_root: &Path) -> &Path {
    if !cfg!(windows) && package_root.file_name().is_some_and(|name| name == "lib") {
        package_root.parent().unwrap_or(package_root)
    } else {
        package_root
    }
}

pub(crate) fn command(executable: &Path) -> Command {
    if cfg!(windows)
        && executable
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ps1"))
    {
        let mut command = Command::new("powershell.exe");
        command.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-File"]).arg(executable);
        command
    } else {
        Command::new(executable)
    }
}
