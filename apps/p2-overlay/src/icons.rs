//! Overlay SVG gövdeleri (24×24): `src/overlay/icons.tsx` (kontur 1.8), görev/not araçlarının satır içi
//! ikonları (TasksWindow.tsx, ProjectNotesPanel.tsx). Çizim `p2_ui::icons::Icon::raw(..).stroke(w)`.

use gpui_kit::prelude::*;
use gpui_kit::{Hsla, Svg, px};
use p2_ui::icons::Icon;

pub const MIC: &str = r#"<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3M9 21h6"/>"#;
pub const TEAM: &str = r#"<circle cx="9" cy="8" r="3.2"/><path d="M3.5 19a5.5 5.5 0 0 1 11 0"/><circle cx="17" cy="9" r="2.4"/><path d="M15.5 14.2a4.6 4.6 0 0 1 5 4.3"/>"#;
pub const CHAT: &str = r#"<path d="M4 6.5A2.5 2.5 0 0 1 6.5 4h11A2.5 2.5 0 0 1 20 6.5v7a2.5 2.5 0 0 1-2.5 2.5H10l-4.5 3.5V16A2.5 2.5 0 0 1 4 13.5z"/><path d="M8 9h8M8 12.5h5"/>"#;
pub const NOTES: &str = r#"<path d="M6 3.5h9l4 4V20a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V4.5a1 1 0 0 1 1-1z"/><path d="M14.5 3.5V8H19M8.5 12h7M8.5 15.5h7"/>"#;
pub const TASKS: &str = r#"<rect x="4" y="4" width="16" height="16" rx="3"/><path d="m8 12.2 2.6 2.6L16.5 9"/>"#;
pub const CAPTURE: &str = r#"<path d="M4 8.5A1.5 1.5 0 0 1 5.5 7H8l1.4-2h5.2L16 7h2.5A1.5 1.5 0 0 1 20 8.5v9a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 17.5z"/><circle cx="12" cy="13" r="3.4"/>"#;
pub const GALLERY: &str = r#"<rect x="3.5" y="5" width="17" height="14" rx="2.5"/><path d="m3.5 16 4.6-4.6a1 1 0 0 1 1.4 0L13 15l2.3-2.3a1 1 0 0 1 1.4 0L20.5 16"/><circle cx="15.5" cy="9" r="1.4"/>"#;
pub const SUMMARY: &str = r#"<rect x="4" y="4" width="16" height="16" rx="3"/><path d="M8 15.5v-3M12 15.5v-6M16 15.5v-4.5"/>"#;
pub const BROWSER: &str = r#"<circle cx="12" cy="12" r="8.5"/><path d="M3.5 12h17M12 3.5c2.6 2.6 3.9 5.4 3.9 8.5s-1.3 5.9-3.9 8.5c-2.6-2.6-3.9-5.4-3.9-8.5s1.3-5.9 3.9-8.5z"/>"#;
pub const PERF: &str = r##"<path d="M4 16.5a8 8 0 1 1 16 0"/><path d="m12 15.5 4-5"/><circle cx="12" cy="16" r="1.4" fill="white" stroke="none"/><path d="M4 19.5h16"/>"##;
pub const SETTINGS: &str = r#"<circle cx="12" cy="12" r="3"/><path d="M12 3.5v2.3M12 18.2v2.3M3.5 12h2.3M18.2 12h2.3M6 6l1.6 1.6M16.4 16.4 18 18M6 18l1.6-1.6M16.4 7.6 18 6"/>"#;
pub const PIN: &str = r#"<path d="M14.5 3.5 20.5 9.5l-2.2.6-3.4 3.4.3 3.6-2 2-4.3-4.3L4 19.9l-.1-.1 4.9-4.9-4.3-4.3 2-2 3.6.3 3.4-3.4z"/>"#;
pub const MINIMIZE: &str = r#"<path d="M6 17h12"/>"#;
pub const MAXIMIZE: &str = r#"<rect x="5" y="5" width="14" height="14" rx="2"/>"#;
pub const RESTORE: &str = r#"<rect x="4" y="8" width="12" height="12" rx="2"/><path d="M8 8V6a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-2"/>"#;
pub const CLOSE: &str = r#"<path d="m6 6 12 12M18 6 6 18"/>"#;
pub const POWER: &str = r#"<path d="M12 3.5v8"/><path d="M7 6.5a8 8 0 1 0 10 0"/>"#;
pub const FOLDER: &str = r#"<path d="M3.5 6.5A1.5 1.5 0 0 1 5 5h4.5l2 2H19a1.5 1.5 0 0 1 1.5 1.5v9A1.5 1.5 0 0 1 19 19H5a1.5 1.5 0 0 1-1.5-1.5z"/>"#;
// TasksWindow.tsx
pub const OPEN: &str = r#"<path d="M14 4h6v6"/><path d="M20 4l-9 9"/><path d="M19 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1h5"/>"#;
pub const PLUS: &str = r#"<path d="M12 5v14M5 12h14"/>"#;
pub const BACK: &str = r#"<path d="M19 12H5M11 6l-6 6 6 6"/>"#;
pub const WINDOW: &str = r#"<rect x="3" y="5" width="13" height="11" rx="2"/><path d="M8 19h11a2 2 0 0 0 2-2V9"/>"#;
pub const LIST: &str = r#"<path d="M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01"/>"#;
pub const BOARD: &str = r#"<rect x="3.5" y="4.5" width="17" height="15" rx="2.5"/><path d="M9.5 4.5v15M14.5 4.5v15"/>"#;
pub const CHEVRON: &str = r#"<path d="m6 9 6 6 6-6"/>"#;
// ProjectNotesPanel.tsx
pub const NOTE_PIN: &str = r#"<path d="M8.5 3.5h7l-1 6.5 3.5 3.5v2h-6v5.5l-.5 1-.5-1V15.5h-6v-2l3.5-3.5z"/>"#;
pub const TRASH: &str = r#"<path d="M4 7h16M9.5 7V4h5v3M6.5 7l1 13.5h9l1-13.5M10 11v6M14 11v6"/>"#;
pub const BOLD: &str = r#"<path d="M7 5h6a4 4 0 0 1 0 8H7V5Z"/><path d="M7 13h7a4 4 0 0 1 0 8H7v-8Z"/>"#;
pub const ITALIC: &str = r#"<path d="M10 5h8M6 19h8M14 5 10 19"/>"#;
pub const FMT_LIST: &str = r#"<path d="M9 6h11M9 12h11M9 18h11"/><path d="M4.5 6h.01M4.5 12h.01M4.5 18h.01"/>"#;
pub const LINK: &str = r#"<path d="M10 13.2a4.6 4.6 0 0 0 6.94.5l2.6-2.6a4.6 4.6 0 0 0-6.5-6.5l-1.5 1.48"/><path d="M14 10.8a4.6 4.6 0 0 0-6.94-.5l-2.6 2.6a4.6 4.6 0 0 0 6.5 6.5l1.48-1.48"/>"#;
// TeamWindow.tsx
pub const PROFILE: &str = r#"<circle cx="12" cy="8.5" r="3.6"/><path d="M5 19.5a7 7 0 0 1 14 0"/>"#;
// BrowserWindow.tsx
pub const NAV_BACK: &str = r#"<path d="M15 5l-7 7 7 7"/>"#;
pub const NAV_FWD: &str = r#"<path d="M9 5l7 7-7 7"/>"#;
pub const HOME: &str = r#"<path d="M4 11.5 12 4.5l8 7"/><path d="M6.5 10v9.5h11V10"/><path d="M10 19.5v-5h4v5"/>"#;
pub const BOOKMARK: &str = r#"<path d="M7 4.5h10a1 1 0 0 1 1 1V20l-6-3.5L6 20V5.5a1 1 0 0 1 1-1z"/>"#;

/// Gövde + kontur + boyut + renk → GPUI svg.
pub fn ic(body: &'static str, stroke: f32, size: f32, color: Hsla) -> Svg {
    Icon::raw(body).stroke(stroke).el().size(px(size)).text_color(color)
}

/// Dolgulu ikon (CallIcons: `fill: currentColor`).
pub fn call(name: &str, size: f32, color: Hsla) -> Svg {
    p2_ui::icons::icon(&format!("call:{name}")).filled().el().size(px(size)).text_color(color)
}

/// `src/ComposerIcons.tsx` ikonları.
pub fn composer(name: &str, stroke: f32, size: f32, color: Hsla) -> Svg {
    p2_ui::icons::icon(name).stroke(stroke).el().size(px(size)).text_color(color)
}

pub fn tool_icon(id: crate::wm::ToolId) -> &'static str {
    use crate::wm::ToolId::*;
    match id {
        Voice => MIC,
        Team => TEAM,
        Chat => CHAT,
        Notes => NOTES,
        Tasks | NewTask => TASKS,
        Capture => GALLERY,
        Summary => SUMMARY,
        Browser => BROWSER,
        Perf => PERF,
        Library => SETTINGS,
    }
}
