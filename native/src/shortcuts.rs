use crate::config::Settings;
pub const ACTIONS: &[(&str, &str, bool, &str)] = &[
    ("search", "Global Search", true, "Ctrl+Alt+Space"),
    ("clipboard", "Open Clipboard", true, ""),
    ("ai", "Open AI", true, ""),
    ("web", "Open Web", true, ""),
    ("emoji", "Open Emoji", true, ""),
    ("dictate", "Dictate", true, "Ctrl+Shift+D"),
    ("capture_ai", "Select screen area for AI", true, ""),
    ("pause_clipboard", "Pause clipboard capture", true, ""),
    ("ignore_copy", "Ignore next copy", true, ""),
    (
        "stop_ai_tools",
        "Stop computer / browser control",
        true,
        "Ctrl+Alt+Backspace",
    ),
    ("stop_voice", "Stop voice playback", true, ""),
    ("cycle_modes", "Cycle search modes", false, "Tab"),
    ("cycle_types", "Cycle file types", false, "Ctrl+Tab"),
    ("pin_clip", "Save clipboard item forever", false, "Ctrl+P"),
    ("edit_clip", "Edit clipboard item", false, "Ctrl+E"),
    ("paste_plain", "Paste clipboard item", false, "Ctrl+Enter"),
    ("new_ai", "New Conversation", false, ""),
    ("read_ai", "Read Aloud", false, ""),
    ("save_search", "Save Search", false, ""),
    ("reveal", "Open containing folder", false, ""),
    ("copy_path", "Copy path", false, ""),
];
impl Settings {
    pub fn binding(&self, action: &str) -> &str {
        match action {
            "search" => &self.hotkey,
            "dictate" => &self.dictation_hotkey,
            "cycle_modes" => &self.llm_hotkey,
            "cycle_types" => &self.type_hotkey,
            _ => self
                .extra_shortcuts
                .get(action)
                .map(String::as_str)
                .unwrap_or_else(|| ACTIONS.iter().find(|a| a.0 == action).map_or("", |a| a.3)),
        }
    }
    pub fn set_binding(&mut self, action: &str, value: String) {
        self.forced_shortcuts.remove(action);
        match action {
            "search" => self.hotkey = value,
            "dictate" => {
                self.dictation_hotkey = value.clone();
                self.voice_search_hotkey = value;
            }
            "cycle_modes" => self.llm_hotkey = value,
            "cycle_types" => self.type_hotkey = value,
            _ => {
                self.extra_shortcuts.insert(action.into(), value);
            }
        }
    }
    pub fn reset_bindings(&mut self) {
        for (id, _, _, default) in ACTIONS {
            self.set_binding(id, (*default).into());
        }
    }
    pub fn binding_conflict(&self, action: &str, value: &str) -> Option<&'static str> {
        ACTIONS
            .iter()
            .find(|a| {
                a.0 != action
                    && !value.is_empty()
                    && canonical(self.binding(a.0)) == canonical(value)
            })
            .map(|a| a.0)
    }
    pub fn assign_binding(
        &mut self,
        action: &str,
        value: String,
        override_old: bool,
    ) -> Result<(), String> {
        if let Some(old) = self.binding_conflict(action, &value) {
            if !override_old {
                return Err(old.into());
            }
            let conflicts = ACTIONS
                .iter()
                .filter(|a| a.0 != action && canonical(self.binding(a.0)) == canonical(&value))
                .map(|a| a.0)
                .collect::<Vec<_>>();
            for old in conflicts {
                self.set_binding(old, String::new());
            }
        }
        self.set_binding(action, canonical(&value));
        Ok(())
    }
    pub fn global_bindings(&self) -> Vec<(String, String)> {
        ACTIONS
            .iter()
            .filter(|a| a.2 && !self.binding(a.0).is_empty())
            .map(|a| (a.0.into(), self.binding(a.0).into()))
            .collect()
    }
}
pub fn matches(value: &str, binding: &str, backwards: bool) -> bool {
    if binding.is_empty() {
        return false;
    }
    let value = canonical(value);
    let binding = canonical(binding);
    value == binding || backwards && value.replace("Shift+", "") == binding
}
pub fn canonical(value: &str) -> String {
    let parts = value
        .split('+')
        .map(|s| s.trim().to_lowercase())
        .collect::<Vec<_>>();
    let mut out = Vec::new();
    for (key, label) in [
        ("ctrl", "Ctrl"),
        ("alt", "Alt"),
        ("shift", "Shift"),
        ("win", "Win"),
    ] {
        if parts
            .iter()
            .any(|p| p == key || key == "win" && p == "windows")
        {
            out.push(label.to_string());
        }
    }
    if let Some(key) = parts.iter().find(|p| {
        !matches!(
            p.as_str(),
            "ctrl" | "alt" | "shift" | "win" | "windows" | ""
        )
    }) {
        let normalized = [
            "ArrowUp",
            "ArrowDown",
            "ArrowLeft",
            "ArrowRight",
            "PageUp",
            "PageDown",
            "Backspace",
            "Escape",
            "Enter",
            "Space",
            "Tab",
            "Delete",
            "Insert",
            "Home",
            "End",
        ]
        .iter()
        .find(|k| k.to_lowercase() == *key);
        if let Some(key) = normalized {
            out.push((*key).to_string());
            return out.join("+");
        }
        let mut chars = key.chars();
        out.push(
            chars
                .next()
                .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default(),
        );
    }
    out.join("+")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn override_reset_and_windows_key() {
        let mut s = Settings::default();
        s.assign_binding("ai", "Windows+Alt+A".into(), false)
            .unwrap();
        assert_eq!(s.binding("ai"), "Alt+Win+A");
        assert!(s.assign_binding("web", "Win+Alt+A".into(), false).is_err());
        s.assign_binding("web", "Win+Alt+A".into(), true).unwrap();
        assert_eq!(s.binding("ai"), "");
        assert_eq!(s.binding("web"), "Alt+Win+A");
        s.reset_bindings();
        assert_eq!(s.hotkey, "Ctrl+Alt+Space");
        assert_eq!(s.binding("web"), "");
    }
}
