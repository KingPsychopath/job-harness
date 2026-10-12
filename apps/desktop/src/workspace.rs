//! Native selection shares CLI path settings; choosing a folder never initializes or resets it.
use std::path::PathBuf;

fn ready(path: PathBuf) -> Option<PathBuf> {
    let path = path.canonicalize().ok()?;
    harness_core::workspace::validate(&path).ok()?;
    Some(path)
}

pub fn select(explicit: Option<PathBuf>) -> anyhow::Result<Option<PathBuf>> {
    let selected = explicit
        .or_else(|| std::env::var_os("JOB_HARNESS_WORKSPACE").map(PathBuf::from))
        .or_else(|| std::env::var_os("JOB_HARNESS_ROOT").map(PathBuf::from))
        .or_else(|| harness_cli::host::resolve(None).ok());
    if let Some(path) = selected.and_then(ready) {
        return Ok(Some(path));
    }
    loop {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Choose an existing Job Harness workspace")
            .pick_folder()
        else {
            return Ok(None);
        };
        if let Some(path) = ready(path) {
            let remember = rfd::MessageDialog::new()
                .set_title("Use this workspace next time?")
                .set_description("Remember only this folder location in your private host settings. Your career records and credentials remain separate.")
                .set_buttons(rfd::MessageButtons::YesNo)
                .show();
            if remember == rfd::MessageDialogResult::Yes
                && harness_cli::host::configure(&path).is_err()
            {
                rfd::MessageDialog::new().set_title("Workspace opened for this session")
                        .set_description("The folder could not be remembered in your host settings. Your workspace is ready and can still be used now.")
                        .set_buttons(rfd::MessageButtons::Ok).show();
            }
            return Ok(Some(path));
        }
        rfd::MessageDialog::new()
            .set_title("This folder is not a ready workspace")
            .set_description("Choose an existing Job Harness workspace. This app will not create, reset or replace data in the folder you selected.")
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inspecting_an_uninitialized_folder_never_creates_a_workspace() {
        let root = tempfile::tempdir().unwrap();
        assert!(ready(root.path().to_owned()).is_none());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
    #[test]
    fn existing_workspace_selection_is_canonical_and_read_only() {
        let root = tempfile::tempdir().unwrap();
        harness_core::workspace::initialize(root.path()).unwrap();
        let before = std::fs::read(root.path().join("config/career-ledger.json")).unwrap();
        assert_eq!(
            ready(root.path().to_owned()).unwrap(),
            root.path().canonicalize().unwrap()
        );
        assert_eq!(
            std::fs::read(root.path().join("config/career-ledger.json")).unwrap(),
            before
        );
    }
}
