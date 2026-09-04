use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-env-changed=OPENVK_BUILD");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=proto/openvk/v1/api.proto");

    let build = std::env::var("OPENVK_BUILD").unwrap_or_else(|_| resolve_build());
    println!("cargo:rustc-env=OPENVK_BUILD={build}");

    prost_build::compile_protos(&["proto/openvk/v1/api.proto"], &["proto"])?;
    Ok(())
}

fn resolve_build() -> String {
    git_output(["rev-parse", "--short=7", "HEAD"]).unwrap_or_else(|| "unknown".into())
}

fn git_output<const N: usize>(args: [&str; N]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let text = text.trim().to_owned();
    if text.is_empty() { None } else { Some(text) }
}
