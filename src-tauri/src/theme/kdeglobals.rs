//! Parsing of `~/.config/kdeglobals`.

use serde::Serialize;

/// Colors reported by the desktop environment.
#[derive(Debug, Clone, Serialize)]
pub struct SystemTheme {
    /// Accent color as `#rrggbb`.
    pub accent: String,
    pub dark: bool,
}

pub(super) fn parse_kdeglobals(text: &str) -> Option<SystemTheme> {
    let mut section = String::new();
    let mut accent = None;
    let mut selection = None;
    let mut window_bg = None;
    let mut dark = None;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            section = line.trim_matches(['[', ']']).to_string();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());

        match (section.as_str(), key) {
            ("General", "AccentColor") => accent = parse_color(value),
            ("Colors:Selection", "BackgroundNormal") => selection = parse_color(value),
            ("Colors:Window", "BackgroundNormal") => window_bg = parse_color(value),
            ("General", "ColorScheme") => dark = Some(value.to_ascii_lowercase().contains("dark")),
            _ => {}
        }
    }

    let accent = accent.or(selection)?;
    let dark = dark.or_else(|| window_bg.map(is_dark)).unwrap_or(false);

    Some(SystemTheme { accent, dark })
}

fn parse_color(value: &str) -> Option<String> {
    if let Some(stripped) = value.strip_prefix('#') {
        if stripped.len() == 6 {
            return Some(format!("#{}", stripped.to_ascii_lowercase()));
        }
        return None;
    }

    let parts: Vec<u8> = value
        .split(',')
        .map(|part| part.trim().parse::<u8>())
        .collect::<Result<_, _>>()
        .ok()?;

    match parts.as_slice() {
        [r, g, b] => Some(format!("#{r:02x}{g:02x}{b:02x}")),
        _ => None,
    }
}

fn is_dark(hex: String) -> bool {
    let channel = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&hex[range], 16).unwrap_or(0) as f32 / 255.0
    };
    let luma = 0.2126 * channel(1..3) + 0.7152 * channel(3..5) + 0.0722 * channel(5..7);
    luma < 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_accent_and_dark_scheme() {
        let text = "\
[General]
AccentColor=100,125,63
ColorScheme=MaterialYouDark

[Colors:Window]
BackgroundNormal=#21251d
";
        let theme = parse_kdeglobals(text).unwrap();
        assert_eq!(theme.accent, "#647d3f");
        assert!(theme.dark);
    }

    #[test]
    fn falls_back_to_selection_color() {
        let text = "\
[Colors:Selection]
BackgroundNormal=10,20,30
";
        let theme = parse_kdeglobals(text).unwrap();
        assert_eq!(theme.accent, "#0a141e");
        assert!(!theme.dark);
    }
}
