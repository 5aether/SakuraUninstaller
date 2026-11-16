use super::runner::Launch;
use crate::utils::paths::system_directory;
use std::{io, path::Path};

pub fn is_product_code(code: &str) -> bool {
    let bytes = code.as_bytes();
    bytes.len() == 38
        && bytes.iter().enumerate().all(|(i, byte)| match i {
            0 => *byte == b'{',
            37 => *byte == b'}',
            9 | 14 | 19 | 24 => *byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

pub fn launch_msi(path: &Path) -> io::Result<()> {
    std::process::Command::new(system_directory()?.join("msiexec.exe"))
        .arg("/i")
        .arg(path)
        .arg("/norestart")
        .spawn()?;
    Ok(())
}

pub(crate) fn removal(code: &str, quiet: bool) -> io::Result<Launch> {
    Ok(Launch {
        executable: system_directory()?.join("msiexec.exe"),
        arguments: format!("/x {code} /norestart{}", if quiet { " /qn" } else { "" }),
    })
}
