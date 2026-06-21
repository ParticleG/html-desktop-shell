use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

const WEB_INDEX: &str = "index.html";
const GENERATED_WEB_DIR: &str = "web-dist";
const LOCAL_WEB_DIR: &str = ".local/share/html-desktop-shell/web";
const XDG_WEB_DIR: &str = "html-desktop-shell/web";
const INSTALLED_WEB_DIR: &str = "/usr/share/html-desktop-shell/web";

pub fn web_index_path() -> Result<PathBuf, String> {
    web_index_path_from_parts(
        std::env::var_os("HTML_DESKTOP_SHELL_WEB_DIR"),
        std::env::current_dir()
            .map_err(|error| format!("failed to resolve current directory: {error}"))?,
        PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        std::env::var_os("XDG_DATA_HOME"),
        std::env::var_os("HOME"),
    )
}

fn web_index_path_from_parts(
    env_web_dir: Option<OsString>,
    current_dir: PathBuf,
    manifest_dir: PathBuf,
    xdg_data_home: Option<OsString>,
    home: Option<OsString>,
) -> Result<PathBuf, String> {
    let candidates =
        web_index_candidates(env_web_dir, current_dir, manifest_dir, xdg_data_home, home);
    for candidate in &candidates {
        if candidate.exists() {
            return Ok(candidate.clone());
        }
    }

    Err(format_missing_web_index(&candidates))
}

fn web_index_candidates(
    env_web_dir: Option<OsString>,
    current_dir: PathBuf,
    manifest_dir: PathBuf,
    xdg_data_home: Option<OsString>,
    home: Option<OsString>,
) -> Vec<PathBuf> {
    let mut candidates = Vec::with_capacity(6);

    if let Some(web_dir) = env_web_dir {
        candidates.push(absolute_web_dir(&current_dir, PathBuf::from(web_dir)).join(WEB_INDEX));
    }

    candidates.push(current_dir.join(GENERATED_WEB_DIR).join(WEB_INDEX));
    candidates.push(manifest_dir.join(GENERATED_WEB_DIR).join(WEB_INDEX));
    if let Some(xdg_data_home) = xdg_data_home {
        candidates.push(
            PathBuf::from(xdg_data_home)
                .join(XDG_WEB_DIR)
                .join(WEB_INDEX),
        );
    }
    if let Some(home) = home {
        candidates.push(PathBuf::from(home).join(LOCAL_WEB_DIR).join(WEB_INDEX));
    }
    candidates.push(Path::new(INSTALLED_WEB_DIR).join(WEB_INDEX));

    candidates
}

fn absolute_web_dir(current_dir: &Path, web_dir: PathBuf) -> PathBuf {
    if web_dir.is_absolute() {
        web_dir
    } else {
        current_dir.join(web_dir)
    }
}

fn format_missing_web_index(candidates: &[PathBuf]) -> String {
    let checked_paths = candidates
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!("missing web assets index.html: checked {checked_paths}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "html-desktop-shell-assets-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn write_index(web_dir: &Path) {
        fs::create_dir_all(web_dir).expect("web dir should be created");
        fs::write(web_dir.join(WEB_INDEX), "<!doctype html>").expect("index should be written");
    }

    fn resolve_web_index(
        env_web_dir: Option<OsString>,
        current_dir: PathBuf,
        manifest_dir: PathBuf,
    ) -> Result<PathBuf, String> {
        web_index_path_from_parts(env_web_dir, current_dir, manifest_dir, None, None)
    }

    #[test]
    fn env_web_dir_precedes_current_and_manifest_web_dirs() {
        let root = temp_root("env-first");
        let env_dir = root.join("env-web");
        let current_dir = root.join("current");
        let manifest_dir = root.join("manifest");
        write_index(&env_dir);
        write_index(&current_dir.join(GENERATED_WEB_DIR));
        write_index(&manifest_dir.join(GENERATED_WEB_DIR));

        let path = resolve_web_index(
            Some(env_dir.clone().into_os_string()),
            current_dir,
            manifest_dir,
        )
        .expect("env web dir should resolve");

        assert_eq!(path, env_dir.join(WEB_INDEX));
    }

    #[test]
    fn relative_env_web_dir_is_resolved_from_current_dir() {
        let root = temp_root("relative-env");
        let current_dir = root.join("current");
        write_index(&current_dir.join("assets"));

        let path = resolve_web_index(
            Some(OsString::from("assets")),
            current_dir.clone(),
            root.join("manifest"),
        )
        .expect("relative env web dir should resolve");

        assert_eq!(path, current_dir.join("assets").join(WEB_INDEX));
    }

    #[test]
    fn current_web_dir_precedes_manifest_web_dir() {
        let root = temp_root("current-first");
        let current_dir = root.join("current");
        let manifest_dir = root.join("manifest");
        write_index(&current_dir.join(GENERATED_WEB_DIR));
        write_index(&manifest_dir.join(GENERATED_WEB_DIR));

        let path = resolve_web_index(None, current_dir.clone(), manifest_dir)
            .expect("current web dir should resolve");

        assert_eq!(path, current_dir.join(GENERATED_WEB_DIR).join(WEB_INDEX));
    }

    #[test]
    fn xdg_data_web_dir_precedes_installed_web_dir() {
        let root = temp_root("xdg-data");
        let current_dir = root.join("current");
        let manifest_dir = root.join("manifest");
        let xdg_data = root.join("xdg-data");
        write_index(&xdg_data.join(XDG_WEB_DIR));

        let path = web_index_path_from_parts(
            None,
            current_dir,
            manifest_dir,
            Some(xdg_data.clone().into_os_string()),
            Some(root.join("home").into_os_string()),
        )
        .expect("xdg data web dir should resolve");

        assert_eq!(path, xdg_data.join(XDG_WEB_DIR).join(WEB_INDEX));
    }

    #[test]
    fn missing_error_lists_all_checked_paths() {
        let root = temp_root("missing");
        let env_dir = root.join("env-web");
        let current_dir = root.join("current");
        let manifest_dir = root.join("manifest");

        let error = web_index_path_from_parts(
            Some(env_dir.clone().into_os_string()),
            current_dir.clone(),
            manifest_dir.clone(),
            Some(root.join("xdg-data").into_os_string()),
            Some(root.join("home").into_os_string()),
        )
        .expect_err("missing files should error");

        assert!(error.contains(env_dir.join(WEB_INDEX).to_string_lossy().as_ref()));
        assert!(
            error.contains(
                current_dir
                    .join(GENERATED_WEB_DIR)
                    .join(WEB_INDEX)
                    .to_string_lossy()
                    .as_ref()
            )
        );
        assert!(
            error.contains(
                manifest_dir
                    .join(GENERATED_WEB_DIR)
                    .join(WEB_INDEX)
                    .to_string_lossy()
                    .as_ref()
            )
        );
        assert!(
            error.contains(
                root.join("xdg-data")
                    .join(XDG_WEB_DIR)
                    .join(WEB_INDEX)
                    .to_string_lossy()
                    .as_ref()
            )
        );
        assert!(
            error.contains(
                root.join("home")
                    .join(LOCAL_WEB_DIR)
                    .join(WEB_INDEX)
                    .to_string_lossy()
                    .as_ref()
            )
        );
        assert!(error.contains(INSTALLED_WEB_DIR));
    }
}
