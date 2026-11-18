pub(crate) const SETUP_VERSION: &str = "Inno Setup: Setup Version";
pub(crate) const APP_PATH: &str = "Inno Setup: App Path";

pub(crate) fn quiet_arguments(arguments: &str) -> String {
    format!("/VERYSILENT /SUPPRESSMSGBOXES /NORESTART {arguments}")
        .trim_end()
        .to_owned()
}
