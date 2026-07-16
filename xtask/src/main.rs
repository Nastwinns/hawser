//! Release/packaging automation: `cargo xtask dist` builds a release binary,
//! archives it under `dist/`, and prints its SHA-256 for the Homebrew formula
//! and Scoop manifest in `packaging/`.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let task = args.next().unwrap_or_default();
    match task.as_str() {
        "dist" => {
            let target = parse_target(args)?;
            dist(target)
        }
        _ => {
            eprintln!("tasks:\n  dist [--target <triple>]  build a release archive under dist/");
            Ok(())
        }
    }
}

fn parse_target(mut args: impl Iterator<Item = String>) -> Result<Option<String>> {
    let mut target = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--target" => {
                let value = args.next().context("--target requires a triple argument")?;
                target = Some(value);
            }
            other => {
                if let Some(value) = other.strip_prefix("--target=") {
                    target = Some(value.to_string());
                } else {
                    bail!("unknown argument: {other}");
                }
            }
        }
    }
    Ok(target)
}

fn workspace_root() -> Result<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(Path::to_path_buf)
        .context("xtask lives one level under the workspace root")
}

fn run(cmd: &mut Command, what: &str) -> Result<String> {
    let output = cmd.output().with_context(|| format!("running {what}"))?;
    if !output.status.success() {
        bail!(
            "{what} failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn host_triple() -> Result<String> {
    let verbose = run(Command::new("rustc").arg("-vV"), "rustc -vV")?;
    verbose
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(str::to_string)
        .context("no host triple in rustc -vV")
}

fn sha256(path: &Path) -> Option<String> {
    let unix = Command::new("shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let out = unix.or_else(|| {
        Command::new("certutil")
            .arg("-hashfile")
            .arg(path)
            .arg("SHA256")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    })?;
    out.split_whitespace()
        .find(|token| token.len() == 64 && token.chars().all(|c| c.is_ascii_hexdigit()))
        .map(str::to_string)
}

fn dist(target: Option<String>) -> Result<()> {
    let root = workspace_root()?;
    let version = env!("CARGO_PKG_VERSION");
    let triple = match target {
        Some(t) => t,
        None => host_triple()?,
    };

    println!("building haw {version} ({triple})…");
    run(
        Command::new("cargo")
            .args(["build", "--release", "-p", "hawser", "--target", &triple])
            .current_dir(&root),
        "cargo build --release",
    )?;

    let binary = if triple.contains("windows") {
        "haw.exe"
    } else {
        "haw"
    };
    let release_dir = root.join("target").join(&triple).join("release");
    let built = release_dir.join(binary);
    if !built.exists() {
        bail!("release binary missing at {}", built.display());
    }

    let dist = root.join("dist");
    std::fs::create_dir_all(&dist)?;
    let archive = dist.join(if triple.contains("windows") {
        format!("haw-{version}-{triple}.zip")
    } else {
        format!("haw-{version}-{triple}.tar.gz")
    });
    let _ = std::fs::remove_file(&archive);

    let mut tar = Command::new("tar");
    if triple.contains("windows") {
        tar.arg("-a").arg("-c").arg("-f");
    } else {
        tar.arg("-czf");
    }
    run(
        tar.arg(&archive).arg(binary).current_dir(&release_dir),
        "tar",
    )?;

    println!("wrote {}", archive.display());
    match sha256(&archive) {
        Some(digest) => {
            let sidecar = archive.with_file_name(format!(
                "{}.sha256",
                archive
                    .file_name()
                    .and_then(|n| n.to_str())
                    .context("archive path has no file name")?
            ));
            std::fs::write(&sidecar, format!("{digest}\n"))?;
            println!("wrote {}", sidecar.display());
            println!("sha256  {digest}");
            println!(
                "render manifests: python3 packaging/render.py {version} \
                 <sha_macos_arm64> <sha_macos_x64> <sha_linux_x64> <sha_windows_x64>"
            );
            println!("  -> writes dist/hawser.rb (Homebrew) and dist/hawser.json (Scoop)");
        }
        None => println!("(no shasum/certutil found — compute the sha256 yourself)"),
    }
    Ok(())
}
