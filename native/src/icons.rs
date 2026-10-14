use eframe::egui::{self, Color32};
#[derive(Clone, Copy)]
pub enum Icon {
    Search,
    Sources,
    Models,
    Schedule,
    Settings,
    Logs,
    About,
    Robot,
    Emoji,
    Clipboard,
    Microphone,
    Speaker,
    Camera,
    Image,
    File,
    Menu,
    Close,
    Minimize,
    Maximize,
    Restore,
}
pub fn image(icon: Icon, color: Color32, size: f32) -> egui::Image<'static> {
    let (name, bytes): (&str, &[u8]) = match icon {
        Icon::Search => ("magnify", include_bytes!("../assets/mdi/magnify.svg")),
        Icon::Sources => (
            "folder-multiple",
            include_bytes!("../assets/mdi/folder-multiple.svg"),
        ),
        Icon::Models => ("chip", include_bytes!("../assets/mdi/chip.svg")),
        Icon::Schedule => (
            "calendar-clock",
            include_bytes!("../assets/mdi/calendar-clock.svg"),
        ),
        Icon::Settings => ("cog", include_bytes!("../assets/mdi/cog.svg")),
        Icon::Logs => (
            "text-box-outline",
            include_bytes!("../assets/mdi/text-box-outline.svg"),
        ),
        Icon::Emoji => (
            "emoticon-happy-outline",
            include_bytes!("../assets/mdi/emoticon-happy-outline.svg"),
        ),
        Icon::Clipboard => (
            "clipboard-outline",
            include_bytes!("../assets/mdi/clipboard-outline.svg"),
        ),
        Icon::Speaker => (
            "volume-high",
            include_bytes!("../assets/mdi/volume-high.svg"),
        ),
        Icon::Microphone => ("microphone", include_bytes!("../assets/mdi/microphone.svg")),
        Icon::Camera => ("camera", include_bytes!("../assets/mdi/camera.svg")),
        Icon::Robot => ("robot-love", include_bytes!("../assets/mdi/robot-love.svg")),
        Icon::About => (
            "information-outline",
            include_bytes!("../assets/mdi/information-outline.svg"),
        ),
        Icon::Image => ("image", include_bytes!("../assets/mdi/image.svg")),
        Icon::File => (
            "text-box-outline",
            include_bytes!("../assets/mdi/text-box-outline.svg"),
        ),
        Icon::Menu => ("menu", include_bytes!("../assets/mdi/menu.svg")),
        Icon::Close => ("close", include_bytes!("../assets/mdi/close.svg")),
        Icon::Minimize => (
            "window-minimize",
            include_bytes!("../assets/mdi/window-minimize.svg"),
        ),
        Icon::Maximize => (
            "window-maximize",
            include_bytes!("../assets/mdi/window-maximize.svg"),
        ),
        Icon::Restore => (
            "window-restore",
            include_bytes!("../assets/mdi/window-restore.svg"),
        ),
    };
    egui::Image::from_bytes(format!("bytes://mdi-{name}.svg"), bytes)
        .fit_to_exact_size(egui::vec2(size, size))
        .tint(color)
}
pub fn result(kind: &str, path: &str) -> Icon {
    if kind == "settings" {
        Icon::Settings
    } else if kind == "folder" {
        Icon::Sources
    } else if crate::content::is_image(&crate::content::extension(std::path::Path::new(path))) {
        Icon::Image
    } else {
        Icon::File
    }
}
