use std::path::PathBuf;
use std::process::Command;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(|s| s.as_str()) {
        Some("install") => install(),
        Some(cmd) => {
            eprintln!("Unknown command: {}", cmd);
            eprintln!("Available commands: install");
            std::process::exit(1);
        }
        None => {
            eprintln!("Usage: cargo xtask <command>");
            eprintln!("Available commands: install");
            std::process::exit(1);
        }
    }
}

fn install() {
    // Find workspace root by looking for Cargo.toml with [workspace]
    let workspace_root = find_workspace_root().expect("Could not find workspace root");

    // dibs-cli is excluded from workspace, so build it directly from its directory
    let dibs_cli_dir = workspace_root.join("crates/dibs-cli");

    println!("Building dibs-cli...");
    let status = Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(&dibs_cli_dir)
        .status()
        .expect("Failed to run cargo build");

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    // dibs-cli has its own target directory since it's excluded from workspace
    let src = dibs_cli_dir.join("target/release/dibs");

    // Copy to ~/.cargo/bin
    let home = std::env::var("HOME").expect("HOME not set");
    let dst = format!("{}/.cargo/bin/dibs", home);

    std::fs::copy(&src, &dst).expect("Failed to copy binary");
    println!("Copied dibs to {}", dst);

    // On macOS, codesign the installed binary to avoid AMFI issues
    // (signing must happen AFTER copy, not before)
    #[cfg(target_os = "macos")]
    {
        println!("Signing installed binary...");
        let status = Command::new("codesign")
            .args(["--sign", "-", "--force", &dst])
            .status()
            .expect("Failed to run codesign");

        if !status.success() {
            eprintln!("Warning: codesign failed, continuing anyway");
        }
    }

    // Verify the installed binary works
    println!("Verifying installation...");
    let output = Command::new(&dst)
        .arg("--version")
        .output()
        .expect("Failed to run dibs --version");

    if !output.status.success() {
        eprintln!("Error: dibs --version failed");
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
        std::process::exit(1);
    }

    let version = String::from_utf8_lossy(&output.stdout);
    println!("Installed: {}", version.trim());
}

/// Find the workspace root by walking up from the current directory
/// looking for a Cargo.toml with `[workspace]`.
fn find_workspace_root() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;

    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists()
            && let Ok(contents) = std::fs::read_to_string(&cargo_toml)
            && contents.contains("[workspace]")
        {
            return Some(dir);
        }

        if !dir.pop() {
            return None;
        }
    }
}
