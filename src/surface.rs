#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SurfaceKind {
    Bar,
    QuickPanel,
    WorkspaceHud,
    DiagnosticsDialog,
    TrayMenu,
}

impl SurfaceKind {
    pub const ALL: [Self; 5] = [
        Self::Bar,
        Self::QuickPanel,
        Self::WorkspaceHud,
        Self::DiagnosticsDialog,
        Self::TrayMenu,
    ];

    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Bar => "bar",
            Self::QuickPanel => "quickPanel",
            Self::WorkspaceHud => "workspaceHud",
            Self::DiagnosticsDialog => "diagnosticsDialog",
            Self::TrayMenu => "trayMenu",
        }
    }

    pub const fn namespace_prefix(self) -> &'static str {
        match self {
            Self::Bar => "html-desktop-shell-panel",
            Self::QuickPanel => "html-desktop-shell-quick-panel",
            Self::WorkspaceHud => "html-desktop-shell-workspace-hud",
            Self::DiagnosticsDialog => "html-desktop-shell-diagnostics",
            Self::TrayMenu => "html-desktop-shell-tray-menu",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SurfaceKey {
    pub kind: SurfaceKind,
    pub monitor_index: u32,
}

impl SurfaceKey {
    pub const fn new(kind: SurfaceKind, monitor_index: u32) -> Self {
        Self {
            kind,
            monitor_index,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(tag = "kind")]
pub enum SurfaceContext {
    #[serde(rename = "bar")]
    Bar {
        #[serde(rename = "monitorIndex")]
        monitor_index: u32,
        output: Option<String>,
    },
    #[serde(rename = "quickPanel")]
    QuickPanel {
        #[serde(rename = "monitorIndex")]
        monitor_index: u32,
        output: Option<String>,
    },
    #[serde(rename = "workspaceHud")]
    WorkspaceHud {
        #[serde(rename = "monitorIndex")]
        monitor_index: u32,
        output: Option<String>,
        #[serde(rename = "workspaceIndex")]
        workspace_index: u64,
    },
    #[serde(rename = "diagnosticsDialog")]
    DiagnosticsDialog {
        #[serde(rename = "monitorIndex")]
        monitor_index: u32,
        output: Option<String>,
    },
    #[serde(rename = "trayMenu")]
    TrayMenu {
        #[serde(rename = "monitorIndex")]
        monitor_index: u32,
        output: Option<String>,
        #[serde(rename = "trayItemId")]
        tray_item_id: String,
    },
}

impl SurfaceContext {
    pub const fn kind(&self) -> SurfaceKind {
        match self {
            Self::Bar { .. } => SurfaceKind::Bar,
            Self::QuickPanel { .. } => SurfaceKind::QuickPanel,
            Self::WorkspaceHud { .. } => SurfaceKind::WorkspaceHud,
            Self::DiagnosticsDialog { .. } => SurfaceKind::DiagnosticsDialog,
            Self::TrayMenu { .. } => SurfaceKind::TrayMenu,
        }
    }

    pub const fn monitor_index(&self) -> u32 {
        match self {
            Self::Bar { monitor_index, .. }
            | Self::QuickPanel { monitor_index, .. }
            | Self::WorkspaceHud { monitor_index, .. }
            | Self::DiagnosticsDialog { monitor_index, .. }
            | Self::TrayMenu { monitor_index, .. } => *monitor_index,
        }
    }

    pub fn output(&self) -> Option<&str> {
        match self {
            Self::Bar { output, .. }
            | Self::QuickPanel { output, .. }
            | Self::WorkspaceHud { output, .. }
            | Self::DiagnosticsDialog { output, .. }
            | Self::TrayMenu { output, .. } => output.as_deref(),
        }
    }

    pub const fn key(&self) -> SurfaceKey {
        SurfaceKey::new(self.kind(), self.monitor_index())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SurfaceError {
    Unavailable { monitor_index: u32 },
    Failed(String),
}

impl SurfaceError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Unavailable { .. } => "surface_unavailable",
            Self::Failed(_) => "surface_failed",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Unavailable { monitor_index } => {
                format!("monitor {monitor_index} is no longer available")
            }
            Self::Failed(message) => message.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_each_surface_context_with_exact_wire_shape() {
        let cases = [
            (
                SurfaceContext::Bar {
                    monitor_index: 1,
                    output: Some("DP-1".to_owned()),
                },
                serde_json::json!({
                    "kind": "bar",
                    "monitorIndex": 1,
                    "output": "DP-1",
                }),
            ),
            (
                SurfaceContext::QuickPanel {
                    monitor_index: 2,
                    output: None,
                },
                serde_json::json!({
                    "kind": "quickPanel",
                    "monitorIndex": 2,
                    "output": null,
                }),
            ),
            (
                SurfaceContext::WorkspaceHud {
                    monitor_index: 3,
                    output: Some("HDMI-A-1".to_owned()),
                    workspace_index: 9,
                },
                serde_json::json!({
                    "kind": "workspaceHud",
                    "monitorIndex": 3,
                    "output": "HDMI-A-1",
                    "workspaceIndex": 9,
                }),
            ),
            (
                SurfaceContext::DiagnosticsDialog {
                    monitor_index: 4,
                    output: None,
                },
                serde_json::json!({
                    "kind": "diagnosticsDialog",
                    "monitorIndex": 4,
                    "output": null,
                }),
            ),
            (
                SurfaceContext::TrayMenu {
                    monitor_index: 5,
                    output: Some("DP-3".to_owned()),
                    tray_item_id: "tray-1-8".to_owned(),
                },
                serde_json::json!({
                    "kind": "trayMenu",
                    "monitorIndex": 5,
                    "output": "DP-3",
                    "trayItemId": "tray-1-8",
                }),
            ),
        ];

        for (context, expected) in cases {
            assert_eq!(serde_json::to_value(context).unwrap(), expected);
        }
    }

    #[test]
    fn context_accessors_preserve_typed_identity() {
        let context = SurfaceContext::WorkspaceHud {
            monitor_index: 7,
            output: Some("eDP-1".to_owned()),
            workspace_index: 5,
        };

        assert_eq!(context.kind(), SurfaceKind::WorkspaceHud);
        assert_eq!(context.key(), SurfaceKey::new(SurfaceKind::WorkspaceHud, 7));
        assert_eq!(context.output(), Some("eDP-1"));
    }
}
