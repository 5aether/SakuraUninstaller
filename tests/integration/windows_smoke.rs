#![cfg(windows)]

#[test]
fn executable_runs_the_read_only_smoke_test() -> std::io::Result<()> {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_SakuraUninstaller"))
        .arg("--smoke-test")
        .output()?;
    assert!(
        output.status.success(),
        "smoke test failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
