use super::{Hive, Integration, autorun};
use std::{fs, io};
use windows_registry::CURRENT_USER;

#[test]
fn matches_startup_commands_only_inside_the_app_directory() -> io::Result<()> {
    struct Fixture(String);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = CURRENT_USER.remove_tree(&self.0);
        }
    }
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("Sakura fixture");
    let sibling = temp.path().join("Sakura fixture other");
    fs::create_dir(&root)?;
    fs::create_dir(&sibling)?;
    let fixture = Fixture(format!(
        r"Software\SakuraStartupFixture-{}",
        uuid::Uuid::new_v4()
    ));
    let key = CURRENT_USER.create(&fixture.0).map_err(io::Error::other)?;
    for directory in [&root, &sibling] {
        let target = directory.join("app.exe");
        fs::write(&target, [])?;
        key.set_string("Fixture", format!(r#""{}" --startup"#, target.display()))
            .map_err(io::Error::other)?;
        let value = key.get_value("Fixture").map_err(io::Error::other)?;
        let normalized = crate::utils::paths::comparable(&root);
        let item = autorun(&normalized, Hive::User, &fixture.0, "Fixture".into(), value);
        if directory == &root {
            assert!(
                matches!(item, Some(Integration::Startup { target: found, .. }) if found == target)
            );
        } else {
            assert!(item.is_none());
        }
    }
    Ok(())
}
