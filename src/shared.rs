use clientele::Utf8PathBuf;
use miette::{Result, miette};
use std::format;

pub fn get_data_dir() -> Result<clientele::Utf8PathBuf> {
    const MODULE_NAME: &str = "asimov-telegram-module";

    #[cfg(unix)]
    return clientele::paths::xdg_data_home().map(|p| p.join(MODULE_NAME)).ok_or_else(|| miette!(
            "Unable to determine a directory for data. Neither $XDG_DATA_HOME nor $HOME available."
        ));

    #[cfg(windows)]
    return clientele::envs::windows::appdata()
        .map(|p| Utf8PathBuf::from(p).join(MODULE_NAME))
        .ok_or_else(|| {
            miette!("Unable to determine a directory for data. %APPDATA% is not available.")
        });
}
