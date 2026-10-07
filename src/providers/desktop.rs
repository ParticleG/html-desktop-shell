#[derive(Clone, Copy, Debug)]
pub(crate) struct ScreenPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TrayOrientation {
    Horizontal,
    Vertical,
}

impl TrayOrientation {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }
}

#[derive(Debug)]
pub(crate) enum DesktopRequest {
    SetAudioVolume {
        volume: u32,
    },
    SetAudioMuted {
        muted: bool,
    },
    GetTrayMenu {
        item_id: String,
        menu_item_id: i32,
    },
    ActivateTrayItem {
        item_id: String,
    },
    SecondaryActivateTrayItem {
        item_id: String,
    },
    ShowTrayItemContextMenu {
        item_id: String,
    },
    ScrollTrayItem {
        item_id: String,
        delta: i32,
        orientation: TrayOrientation,
    },
    ActivateTrayMenuItem {
        item_id: String,
        menu_item_id: i32,
        revision: u32,
    },
    GetNotifications,
    DismissNotification {
        notification_id: u32,
    },
    InvokeNotificationAction {
        notification_id: u32,
        action_key: String,
    },
    MarkAllNotificationsRead,
    SearchApplications {
        query: String,
        limit: u32,
    },
    LaunchApplication {
        application_id: String,
    },
}
