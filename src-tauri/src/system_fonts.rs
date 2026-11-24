//! The font families installed on this machine, for the font pickers in
//! Settings > Appearance. The webview can draw any installed family by name, but
//! WebKit has no API to list them (`queryLocalFonts` is Chromium-only), so the
//! list comes from here: fontdb finds the files in the OS font folders (and,
//! on Linux, the fontconfig setup), ttf-parser reads what each one can draw.

use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::OnceLock;

#[derive(Clone, Serialize)]
pub struct SystemFont {
    pub family: String,
    /// Every face of the family is fixed-width.
    pub mono: bool,
}

static FONTS: OnceLock<Vec<SystemFont>> = OnceLock::new();

/// What a face can draw, read from its tables: `None` when it cannot be read
/// or has no Unicode Latin letters (emoji, symbol, dingbat and CJK-only faces,
/// which would draw a font's name in pictures), else whether it is fixed-width.
/// The fixed-pitch flag alone is wrong both ways (FreeMono lacks it), so equal
/// advances for `i` and `M` count too.
fn latin_face(data: &[u8], index: u32) -> Option<bool> {
    let face = ttf_parser::Face::parse(data, index).ok()?;
    // glyph_index only reads Unicode cmap subtables: a symbol-encoded font
    // (Standard Symbols, the URW dingbats) has none for these.
    let glyph = |c: char| face.glyph_index(c);
    let (i, m) = (glyph('i')?, glyph('M')?);
    glyph('0')?;
    // Some symbol fonts map ASCII too, to other shapes (Standard Symbols puts
    // alpha at `a`). A named glyph for `a` must be named for it; a font without
    // glyph names (post table version 3) is taken at its cmap's word.
    let a = glyph('a')?;
    if let Some(name) = face.glyph_name(a) {
        if !(name == "a" || name.starts_with("a.") || name.eq_ignore_ascii_case("uni0061")) {
            return None;
        }
    }
    let advance = |g| face.glyph_hor_advance(g).unwrap_or(0);
    Some(face.is_monospaced() || (advance(i) > 0 && advance(i) == advance(m)))
}

fn scan() -> Vec<SystemFont> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    // Family name -> fixed-width, judged from its first readable face (the
    // regular one in practice; a family's faces share their pitch).
    let mut by_family: BTreeMap<String, Option<bool>> = BTreeMap::new();
    for face in db.faces() {
        // The first name is the English (or only) one, the one CSS matches.
        let Some((family, _)) = face.families.first() else { continue };
        let family = family.trim();
        // Dot-prefixed families are macOS's private system faces (.SF NS, ...):
        // WebKit refuses them by name, and SF Pro is offered as "System UI".
        if family.is_empty() || family.starts_with('.') {
            continue;
        }
        if matches!(by_family.get(family), Some(Some(_))) {
            continue;
        }
        let judged = db.with_face_data(face.id, latin_face).flatten();
        by_family.insert(family.to_string(), judged);
    }
    by_family
        .into_iter()
        .filter_map(|(family, mono)| mono.map(|mono| SystemFont { family, mono }))
        .collect()
}

fn sorted(mut fonts: Vec<SystemFont>) -> Vec<SystemFont> {
    fonts.sort_by_cached_key(|f| f.family.to_lowercase());
    fonts
}

/// Installed families, sorted by name. Scanned once per launch: reading every
/// font file's header takes a few hundred ms on a machine with many fonts.
#[tauri::command]
pub async fn system_fonts() -> Result<Vec<SystemFont>, String> {
    if let Some(f) = FONTS.get() {
        return Ok(f.clone());
    }
    let fonts = tauri::async_runtime::spawn_blocking(|| sorted(scan()))
        .await
        .map_err(|e| format!("Could not list the installed fonts: {e}"))?;
    Ok(FONTS.get_or_init(|| fonts).clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_by_name_ignoring_case() {
        let fonts = sorted(vec![
            SystemFont { family: "ubuntu".into(), mono: false },
            SystemFont { family: "Adwaita Mono".into(), mono: true },
            SystemFont { family: "Liberation Mono".into(), mono: true },
        ]);
        let names: Vec<_> = fonts.iter().map(|f| f.family.as_str()).collect();
        assert_eq!(names, ["Adwaita Mono", "Liberation Mono", "ubuntu"]);
    }

    #[test]
    fn scan_lists_only_faces_that_draw_latin() {
        let fonts = scan();
        assert!(fonts.iter().all(|f| !f.family.starts_with('.') && !f.family.is_empty()));
        // Where this machine has them: symbol and emoji faces are left out, and
        // FreeMono (no fixed-pitch flag) still counts as fixed-width.
        for hidden in ["Standard Symbols PS", "D050000L", "Noto Color Emoji"] {
            assert!(fonts.iter().all(|f| f.family != hidden), "{hidden} listed");
        }
        if let Some(free) = fonts.iter().find(|f| f.family == "FreeMono") {
            assert!(free.mono);
        }
    }
}
