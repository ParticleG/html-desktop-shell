use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellConfig {
    pub panel_height: i32,
    pub layer: PanelLayer,
    pub keyboard_mode: PanelKeyboardMode,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PanelLayer {
    Top,
    Bottom,
    Overlay,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PanelKeyboardMode {
    None,
    OnDemand,
    Exclusive,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawShellConfig {
    panel_height: Option<i32>,
    layer: Option<PanelLayer>,
    keyboard_mode: Option<PanelKeyboardMode>,
}

#[derive(Debug)]
pub struct LoadedConfig {
    pub config: ShellConfig,
    pub app_args: Vec<String>,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            panel_height: 32,
            layer: PanelLayer::Top,
            keyboard_mode: PanelKeyboardMode::OnDemand,
        }
    }
}

pub fn load() -> Result<LoadedConfig, String> {
    load_from_parts(
        std::env::args_os(),
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

fn load_from_parts<I>(
    args: I,
    xdg_config_home: Option<OsString>,
    home: Option<OsString>,
) -> Result<LoadedConfig, String>
where
    I: IntoIterator<Item = OsString>,
{
    let ParsedArgs {
        config_path,
        app_args,
    } = parse_args(args)?;
    let config = load_config(config_path.as_deref(), xdg_config_home, home)?;

    Ok(LoadedConfig { config, app_args })
}

fn parse_args<I>(args: I) -> Result<ParsedArgs, String>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = args.into_iter();
    let program = args
        .next()
        .unwrap_or_else(|| OsString::from("html-desktop-shell"));
    let mut app_args = vec![program.to_string_lossy().into_owned()];
    let mut config_path = None;

    while let Some(arg) = args.next() {
        if arg == OsStr::new("--config") {
            let Some(path) = args.next() else {
                return Err("--config requires a path".to_owned());
            };
            set_config_path(&mut config_path, PathBuf::from(path))?;
        } else {
            app_args.push(arg.to_string_lossy().into_owned());
        }
    }

    Ok(ParsedArgs {
        config_path,
        app_args,
    })
}

fn set_config_path(slot: &mut Option<PathBuf>, path: PathBuf) -> Result<(), String> {
    if slot.is_some() {
        return Err("duplicate --config argument".to_owned());
    }
    *slot = Some(path);
    Ok(())
}

struct ParsedArgs {
    config_path: Option<PathBuf>,
    app_args: Vec<String>,
}

fn load_config(
    explicit_path: Option<&Path>,
    xdg_config_home: Option<OsString>,
    home: Option<OsString>,
) -> Result<ShellConfig, String> {
    if let Some(path) = explicit_path {
        return read_config_file(path);
    }

    for path in implicit_config_paths(xdg_config_home, home) {
        if path_exists(&path)? {
            return read_config_file(&path);
        }
    }

    Ok(ShellConfig::default())
}

fn implicit_config_paths(
    xdg_config_home: Option<OsString>,
    home: Option<OsString>,
) -> Vec<PathBuf> {
    let mut paths = Vec::with_capacity(2);

    if let Some(xdg_config_home) = xdg_config_home {
        paths.push(
            PathBuf::from(xdg_config_home)
                .join("html-desktop-shell")
                .join("config.toml"),
        );
    }

    if let Some(home) = home {
        paths.push(
            PathBuf::from(home)
                .join(".config")
                .join("html-desktop-shell")
                .join("config.toml"),
        );
    }

    paths
}

fn path_exists(path: &Path) -> Result<bool, String> {
    path.try_exists()
        .map_err(|error| format!("failed to inspect config file {}: {error}", path.display()))
}

fn read_config_file(path: &Path) -> Result<ShellConfig, String> {
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("failed to read config file {}: {error}", path.display()))?;
    parse_config(path, contents.as_str())
}

fn parse_config(path: &Path, contents: &str) -> Result<ShellConfig, String> {
    let raw = toml::from_str::<RawShellConfig>(contents)
        .map_err(|error| format!("invalid config file {}: {error}", path.display()))?;
    let mut config = ShellConfig::default();

    if let Some(panel_height) = raw.panel_height {
        config.panel_height = panel_height;
    }
    if let Some(layer) = raw.layer {
        config.layer = layer;
    }
    if let Some(keyboard_mode) = raw.keyboard_mode {
        config.keyboard_mode = keyboard_mode;
    }

    validate_config(path, &config)?;
    Ok(config)
}

fn validate_config(path: &Path, config: &ShellConfig) -> Result<(), String> {
    if config.panel_height <= 0 {
        return Err(format!(
            "invalid config file {}: panel_height must be greater than 0",
            path.display()
        ));
    }

    Ok(())
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
            "html-desktop-shell-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn write_file(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().expect("test path should have a parent"))
            .expect("test directory should be created");
        fs::write(path, contents).expect("test file should be written");
    }

    #[test]
    fn defaults_without_config_files_preserve_current_behavior() {
        let root = temp_root("defaults");
        let loaded = load_from_parts(
            [OsString::from("html-desktop-shell")],
            Some(root.join("xdg").into_os_string()),
            Some(root.join("home").into_os_string()),
        )
        .expect("missing implicit config should use defaults");

        assert_eq!(loaded.config, ShellConfig::default());
        assert_eq!(loaded.app_args, ["html-desktop-shell"]);
    }

    #[test]
    fn explicit_config_loads_and_is_removed_from_gtk_args() {
        let root = temp_root("explicit");
        let config_path = root.join("panel.toml");
        write_file(
            &config_path,
            r#"
panel_height = 48
layer = "overlay"
keyboard_mode = "exclusive"
"#,
        );

        let loaded = load_from_parts(
            [
                OsString::from("html-desktop-shell"),
                OsString::from("--config"),
                config_path.into_os_string(),
                OsString::from("--gapplication-service"),
            ],
            None,
            None,
        )
        .expect("explicit config should load");

        assert_eq!(loaded.config.panel_height, 48);
        assert_eq!(loaded.config.layer, PanelLayer::Overlay);
        assert_eq!(loaded.config.keyboard_mode, PanelKeyboardMode::Exclusive);
        assert_eq!(
            loaded.app_args,
            ["html-desktop-shell", "--gapplication-service"]
        );
    }

    #[test]
    fn xdg_config_precedes_home_config() {
        let root = temp_root("xdg-precedence");
        let xdg = root.join("xdg");
        let home = root.join("home");
        write_file(
            &xdg.join("html-desktop-shell/config.toml"),
            r#"panel_height = 40"#,
        );
        write_file(
            &home.join(".config/html-desktop-shell/config.toml"),
            r#"panel_height = 56"#,
        );

        let loaded = load_from_parts(
            [OsString::from("html-desktop-shell")],
            Some(xdg.into_os_string()),
            Some(home.into_os_string()),
        )
        .expect("xdg config should load");

        assert_eq!(loaded.config.panel_height, 40);
    }

    #[test]
    fn invalid_existing_config_returns_error() {
        let root = temp_root("invalid");
        let xdg = root.join("xdg");
        write_file(
            &xdg.join("html-desktop-shell/config.toml"),
            r#"panel_height = 0"#,
        );

        let error = load_from_parts(
            [OsString::from("html-desktop-shell")],
            Some(xdg.into_os_string()),
            None,
        )
        .expect_err("invalid config should fail");

        assert!(error.contains("panel_height must be greater than 0"));
    }

    #[test]
    fn duplicate_config_argument_returns_error() {
        let error = load_from_parts(
            [
                OsString::from("html-desktop-shell"),
                OsString::from("--config"),
                OsString::from("one.toml"),
                OsString::from("--config"),
                OsString::from("two.toml"),
            ],
            None,
            None,
        )
        .expect_err("duplicate --config should fail");

        assert_eq!(error, "duplicate --config argument");
    }
}
