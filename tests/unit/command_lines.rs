use super::{Launch, elevation, is_local_absolute, split};
use std::{io, path::Path};

#[test]
fn finds_this_process_by_its_executable_directory() -> io::Result<()> {
    let executable = std::env::current_exe()?;
    let directory = executable.parent().unwrap();
    assert!(
        crate::uninstall::processes::running_under(Some(directory)).contains(&std::process::id())
    );
    let unrelated = tempfile::tempdir()?;
    assert!(
        !crate::uninstall::processes::running_under(Some(unrelated.path()))
            .contains(&std::process::id())
    );
    Ok(())
}

#[test]
fn preserves_installer_arguments_and_does_not_treat_the_path_as_a_command() {
    assert_eq!(
        split(r#""C:\Program Files\Sakura\uninstall.exe" /x "a b""#),
        Some((r"C:\Program Files\Sakura\uninstall.exe", r#"/x "a b""#))
    );
    assert_eq!(
        split(r"C:\Program Files\Sakura\uninstall.exe /S"),
        Some((r"C:\Program Files\Sakura\uninstall.exe", "/S"))
    );
    assert_eq!(split(r#""C:\uninstall.exe"/x"#), None);
    assert!(!is_local_absolute(Path::new(r"\\server\share\remove.exe")));
    assert!(!is_local_absolute(Path::new(r"C:\safe\..\Windows")));
    assert!(!is_local_absolute(Path::new(r"C:\safe\file:ads")));
}

#[test]
fn waits_for_a_real_process_without_uninstalling_software() -> io::Result<()> {
    let launch = Launch {
        executable: std::env::current_exe()?,
        arguments: "--list".into(),
    };
    assert_eq!(elevation::run(&launch)?, 0);
    Ok(())
}
