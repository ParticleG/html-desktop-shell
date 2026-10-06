use std::{
    ffi::OsString,
    fs::File,
    io,
    path::{Path, PathBuf},
};

const WEB_INDEX: &str = "index.html";
const GENERATED_WEB_DIR: &str = "web-dist";
const LOCAL_WEB_DIR: &str = ".local/share/html-desktop-shell/web";
const XDG_WEB_DIR: &str = "html-desktop-shell/web";
const INSTALLED_WEB_DIR: &str = "/usr/share/html-desktop-shell/web";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedWebAssets {
    root: PathBuf,
    index: PathBuf,
}

impl ResolvedWebAssets {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn index(&self) -> &Path {
        &self.index
    }
}

pub fn web_assets() -> Result<ResolvedWebAssets, String> {
    web_assets_from_parts(
        std::env::var_os("HTML_DESKTOP_SHELL_WEB_DIR"),
        std::env::current_dir()
            .map_err(|error| format!("failed to resolve current directory: {error}"))?,
        PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        std::env::var_os("XDG_DATA_HOME"),
        std::env::var_os("HOME"),
    )
}

fn web_assets_from_parts(
    env_web_dir: Option<OsString>,
    current_dir: PathBuf,
    manifest_dir: PathBuf,
    xdg_data_home: Option<OsString>,
    home: Option<OsString>,
) -> Result<ResolvedWebAssets, String> {
    let candidates =
        web_index_candidates(env_web_dir, current_dir, manifest_dir, xdg_data_home, home);
    resolve_web_assets(&candidates)
}

fn resolve_web_assets(candidates: &[PathBuf]) -> Result<ResolvedWebAssets, String> {
    for index in candidates {
        match std::fs::symlink_metadata(index) {
            Ok(_) => return validate_web_assets(index),
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "failed to inspect web assets index {}: {error}",
                    index.display()
                ));
            }
        }
    }

    Err(format_missing_web_index(candidates))
}

fn validate_web_assets(index: &Path) -> Result<ResolvedWebAssets, String> {
    validate_web_assets_with(index, |canonical_index| {
        File::open(canonical_index).map(drop)
    })
}

fn validate_web_assets_with(
    index: &Path,
    open_index: impl FnOnce(&Path) -> io::Result<()>,
) -> Result<ResolvedWebAssets, String> {
    let root = index.parent().ok_or_else(|| {
        format!(
            "web assets index has no parent directory: {}",
            index.display()
        )
    })?;
    let canonical_root = root.canonicalize().map_err(|error| {
        format!(
            "failed to canonicalize web assets root {}: {error}",
            root.display()
        )
    })?;
    let root_metadata = canonical_root.metadata().map_err(|error| {
        format!(
            "failed to inspect canonical web assets root {}: {error}",
            canonical_root.display()
        )
    })?;
    if !root_metadata.is_dir() {
        return Err(format!(
            "web assets root is not a directory: {}",
            root.display()
        ));
    }

    let canonical_index = index.canonicalize().map_err(|error| {
        format!(
            "failed to canonicalize web assets index {}: {error}",
            index.display()
        )
    })?;
    let index_metadata = canonical_index.metadata().map_err(|error| {
        format!(
            "failed to inspect canonical web assets index {}: {error}",
            canonical_index.display()
        )
    })?;
    if !index_metadata.is_file() {
        return Err(format!(
            "web assets index is not a regular file: {}",
            index.display()
        ));
    }
    if !canonical_index.starts_with(&canonical_root) {
        return Err(format!(
            "web assets index escapes its root: {} resolves outside {}",
            index.display(),
            root.display()
        ));
    }
    open_index(&canonical_index).map_err(|error| {
        format!(
            "failed to open web assets index {}: {error}",
            canonical_index.display()
        )
    })?;

    Ok(ResolvedWebAssets {
        root: canonical_root,
        index: canonical_index,
    })
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

    fn resolve_test_web_assets(
        env_web_dir: Option<OsString>,
        current_dir: PathBuf,
        manifest_dir: PathBuf,
    ) -> Result<ResolvedWebAssets, String> {
        web_assets_from_parts(env_web_dir, current_dir, manifest_dir, None, None)
    }

    #[test]
    fn candidates_keep_the_six_tier_order() {
        let root = temp_root("candidate-order");
        let env_dir = root.join("env");
        let current_dir = root.join("current");
        let manifest_dir = root.join("manifest");
        let xdg_data = root.join("xdg");
        let home = root.join("home");

        let candidates = web_index_candidates(
            Some(env_dir.clone().into_os_string()),
            current_dir.clone(),
            manifest_dir.clone(),
            Some(xdg_data.clone().into_os_string()),
            Some(home.clone().into_os_string()),
        );

        assert_eq!(
            candidates,
            vec![
                env_dir.join(WEB_INDEX),
                current_dir.join(GENERATED_WEB_DIR).join(WEB_INDEX),
                manifest_dir.join(GENERATED_WEB_DIR).join(WEB_INDEX),
                xdg_data.join(XDG_WEB_DIR).join(WEB_INDEX),
                home.join(LOCAL_WEB_DIR).join(WEB_INDEX),
                Path::new(INSTALLED_WEB_DIR).join(WEB_INDEX),
            ]
        );
    }

    #[test]
    fn every_candidate_tier_is_selected_before_later_tiers() {
        let root = temp_root("tier-selection");
        let candidates = (0..6)
            .map(|tier| root.join(format!("tier-{tier}")).join(WEB_INDEX))
            .collect::<Vec<_>>();

        for selected_tier in 0..candidates.len() {
            for candidate in &candidates {
                if let Some(web_dir) = candidate.parent() {
                    let _ = fs::remove_dir_all(web_dir);
                }
            }
            for candidate in &candidates[selected_tier..] {
                write_index(candidate.parent().expect("candidate should have a parent"));
            }

            let assets =
                resolve_web_assets(&candidates).expect("the earliest present tier should resolve");
            assert_eq!(
                assets.index(),
                candidates[selected_tier]
                    .canonicalize()
                    .expect("selected index should canonicalize")
            );
        }
    }

    #[test]
    fn relative_env_web_dir_is_resolved_from_current_dir() {
        let root = temp_root("relative-env");
        let current_dir = root.join("current");
        let web_dir = current_dir.join("assets");
        write_index(&web_dir);

        let assets = resolve_test_web_assets(
            Some(OsString::from("assets")),
            current_dir,
            root.join("manifest"),
        )
        .expect("relative env web dir should resolve");

        assert_eq!(
            assets.root(),
            web_dir
                .canonicalize()
                .expect("web root should canonicalize")
        );
        assert_eq!(
            assets.index(),
            web_dir
                .join(WEB_INDEX)
                .canonicalize()
                .expect("web index should canonicalize")
        );
    }

    #[test]
    fn resolved_paths_are_canonical() {
        let root = temp_root("canonical");
        let web_dir = root.join("parent").join("..").join("web");
        write_index(&web_dir);

        let assets = resolve_web_assets(&[web_dir.join(WEB_INDEX)])
            .expect("existing web assets should resolve");

        assert!(assets.root().is_absolute());
        assert!(assets.index().is_absolute());
        assert_eq!(
            assets.root(),
            root.join("web")
                .canonicalize()
                .expect("web root should canonicalize")
        );
        assert!(assets.index().starts_with(assets.root()));
    }

    #[cfg(unix)]
    #[test]
    fn root_symlink_is_allowed_when_index_remains_contained() {
        use std::os::unix::fs::symlink;

        let root = temp_root("root-symlink");
        let actual_root = root.join("actual");
        let linked_root = root.join("linked");
        write_index(&actual_root);
        symlink(&actual_root, &linked_root).expect("root symlink should be created");

        let assets = resolve_web_assets(&[linked_root.join(WEB_INDEX)])
            .expect("contained assets behind a root symlink should resolve");

        assert_eq!(
            assets.root(),
            actual_root
                .canonicalize()
                .expect("actual root should canonicalize")
        );
        assert_eq!(
            assets.index(),
            actual_root
                .join(WEB_INDEX)
                .canonicalize()
                .expect("actual index should canonicalize")
        );
    }

    #[cfg(unix)]
    #[test]
    fn index_symlink_escape_is_rejected_without_falling_back() {
        use std::os::unix::fs::symlink;

        let root = temp_root("index-escape");
        let selected_root = root.join("selected");
        let outside_index = root.join("outside.html");
        let fallback_root = root.join("fallback");
        fs::create_dir_all(&selected_root).expect("selected root should be created");
        fs::write(&outside_index, "<!doctype html>").expect("outside index should be written");
        symlink(&outside_index, selected_root.join(WEB_INDEX))
            .expect("index symlink should be created");
        write_index(&fallback_root);

        let selected_index = selected_root.join(WEB_INDEX);
        let error = resolve_web_assets(&[selected_index.clone(), fallback_root.join(WEB_INDEX)])
            .expect_err("escaping selected index should be rejected");

        assert!(error.contains("escapes its root"));
        assert!(error.contains(selected_index.to_string_lossy().as_ref()));
    }

    #[cfg(unix)]
    #[test]
    fn dangling_index_symlink_is_rejected_without_falling_back() {
        use std::os::unix::fs::symlink;

        let root = temp_root("dangling-index");
        let selected_root = root.join("selected");
        let selected_index = selected_root.join(WEB_INDEX);
        let missing_target = root.join("missing.html");
        let fallback_root = root.join("fallback");
        fs::create_dir_all(&selected_root).expect("selected root should be created");
        symlink(&missing_target, &selected_index).expect("dangling symlink should be created");
        write_index(&fallback_root);

        let error = resolve_web_assets(&[selected_index.clone(), fallback_root.join(WEB_INDEX)])
            .expect_err("dangling selected index should be rejected");

        assert!(error.contains("failed to canonicalize web assets index"));
        assert!(error.contains(selected_index.to_string_lossy().as_ref()));
    }

    #[test]
    fn directory_index_is_rejected_without_falling_back() {
        let root = temp_root("directory-index");
        let selected_index = root.join("selected").join(WEB_INDEX);
        let fallback_root = root.join("fallback");
        fs::create_dir_all(&selected_index).expect("directory index should be created");
        write_index(&fallback_root);

        let error = resolve_web_assets(&[selected_index.clone(), fallback_root.join(WEB_INDEX)])
            .expect_err("directory index should be rejected");

        assert!(error.contains("not a regular file"));
        assert!(error.contains(selected_index.to_string_lossy().as_ref()));
    }

    #[cfg(unix)]
    #[test]
    fn non_regular_index_is_rejected() {
        use std::os::unix::net::UnixListener;

        let root = temp_root("non-regular-index");
        fs::create_dir_all(&root).expect("web root should be created");
        let index = root.join(WEB_INDEX);
        let _listener = UnixListener::bind(&index).expect("socket index should be created");

        let error = resolve_web_assets(std::slice::from_ref(&index))
            .expect_err("socket index should be rejected");

        assert!(error.contains("not a regular file"));
        assert!(error.contains(index.to_string_lossy().as_ref()));
    }

    #[test]
    fn canonicalization_error_names_the_index_path() {
        let root = temp_root("canonicalization-error");
        fs::create_dir_all(&root).expect("web root should be created");
        let index = root.join(WEB_INDEX);

        let error = validate_web_assets(&index)
            .expect_err("a missing exact entry should fail canonicalization");

        assert!(error.contains("failed to canonicalize web assets index"));
        assert!(error.contains(index.to_string_lossy().as_ref()));
    }

    #[test]
    fn exact_canonical_index_is_opened_before_resolution() {
        use std::cell::Cell;

        let root = temp_root("open-index");
        write_index(&root);
        let index = root.join(WEB_INDEX);
        let canonical_index = index.canonicalize().expect("index should canonicalize");
        let opened = Cell::new(false);

        let assets = validate_web_assets_with(&index, |path| {
            assert_eq!(path, canonical_index);
            opened.set(true);
            Ok(())
        })
        .expect("readable index should resolve");

        assert!(opened.get());
        assert_eq!(assets.index(), canonical_index);
    }

    #[test]
    fn index_open_error_names_the_canonical_path() {
        let root = temp_root("open-error");
        write_index(&root);
        let index = root.join(WEB_INDEX);
        let canonical_index = index.canonicalize().expect("index should canonicalize");

        let error = validate_web_assets_with(&index, |_| {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "test read denial",
            ))
        })
        .expect_err("unreadable index should be rejected");

        assert!(error.contains("failed to open web assets index"));
        assert!(error.contains(canonical_index.to_string_lossy().as_ref()));
        assert!(error.contains("test read denial"));
    }

    #[test]
    fn missing_error_lists_all_checked_paths() {
        let root = temp_root("missing");
        let env_dir = root.join("env-web");
        let current_dir = root.join("current");
        let manifest_dir = root.join("manifest");
        let xdg_data = root.join("xdg-data");
        let home = root.join("home");

        let error = web_assets_from_parts(
            Some(env_dir.clone().into_os_string()),
            current_dir.clone(),
            manifest_dir.clone(),
            Some(xdg_data.clone().into_os_string()),
            Some(home.clone().into_os_string()),
        )
        .expect_err("missing files should error");

        for path in [
            env_dir.join(WEB_INDEX),
            current_dir.join(GENERATED_WEB_DIR).join(WEB_INDEX),
            manifest_dir.join(GENERATED_WEB_DIR).join(WEB_INDEX),
            xdg_data.join(XDG_WEB_DIR).join(WEB_INDEX),
            home.join(LOCAL_WEB_DIR).join(WEB_INDEX),
            Path::new(INSTALLED_WEB_DIR).join(WEB_INDEX),
        ] {
            assert!(
                error.contains(path.to_string_lossy().as_ref()),
                "missing error should name {}",
                path.display()
            );
        }
    }
}
