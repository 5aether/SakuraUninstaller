pub(crate) const VERSION: &str = "NSIS:Version";

pub(crate) fn quiet_arguments(arguments: &str) -> String {
    // NSIS requires a case-sensitive /S and any _?= directory to remain last.
    format!("/S {arguments}").trim_end().to_owned()
}
