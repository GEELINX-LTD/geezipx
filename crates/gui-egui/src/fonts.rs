use ab_glyph::{Font, FontRef};
use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use fontdb::{Database, Family, Query};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};

const NOTO: &str = "Noto Sans SC (embedded)";
const CHINESE: &[Family<'_>] = &[
    Family::Name("Microsoft YaHei UI"),
    Family::Name("Microsoft YaHei"),
    Family::Name("PingFang SC"),
    Family::Name("Hiragino Sans GB"),
    Family::Name("Noto Sans CJK SC"),
    Family::Name("Noto Sans SC"),
    Family::Name("Source Han Sans SC"),
    Family::Name("WenQuanYi Micro Hei"),
];

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Selection {
    #[default]
    System,
    Noto,
    Installed(String),
}

impl Selection {
    pub fn label(&self) -> &str {
        match self {
            Self::System => "跟随系统（默认）",
            Self::Noto => "Noto Sans SC（内置）",
            Self::Installed(name) => name,
        }
    }
}

struct Catalog {
    database: Database,
    defaults: FontDefinitions,
    families: Vec<String>,
}

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let mut database = Database::new();
        database.load_system_fonts();
        let defaults = definitions(&database);
        let families: BTreeSet<_> = database
            .faces()
            .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
            .collect();
        Catalog {
            database,
            defaults,
            families: families.into_iter().collect(),
        }
    })
}

pub fn families() -> &'static [String] {
    &catalog().families
}

/// Returns false when a saved or selected system font is no longer usable.
pub fn install(context: &egui::Context, selection: &Selection) -> bool {
    let catalog = catalog();
    let (fonts, available) = selected_definitions(&catalog.database, &catalog.defaults, selection);
    context.set_fonts(fonts);
    available
}

fn selected_definitions(
    database: &Database,
    defaults: &FontDefinitions,
    selection: &Selection,
) -> (FontDefinitions, bool) {
    let mut fonts = defaults.clone();
    let preferred = match selection {
        Selection::System => return (fonts, true),
        Selection::Noto => NOTO.into(),
        Selection::Installed(name) => {
            let Some((name, data)) = resolve(database, &[Family::Name(name)]) else {
                return (fonts, false);
            };
            fonts.font_data.insert(name.clone(), data);
            name
        }
    };
    let family = fonts.families.entry(FontFamily::Proportional).or_default();
    family.retain(|name| *name != preferred);
    family.insert(0, preferred);
    // Keep the default Chinese/symbol fallback chain and a real monospace face for previews.
    (fonts, true)
}

fn ui_families() -> Vec<Family<'static>> {
    #[cfg(target_os = "windows")]
    let names = ["Segoe UI", "Segoe UI Variable"];
    #[cfg(target_os = "macos")]
    let names = [
        ".AppleSystemUIFont",
        ".SF NS Text",
        ".SF NS",
        "SF Pro Text",
        "Helvetica Neue",
    ];
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let names = [];
    let mut families: Vec<_> = names.into_iter().map(Family::Name).collect();
    families.push(Family::SansSerif);
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    families.extend(["Noto Sans", "DejaVu Sans", "Liberation Sans"].map(Family::Name));
    families
}

fn mono_families() -> Vec<Family<'static>> {
    #[cfg(target_os = "windows")]
    let names = ["Consolas"];
    #[cfg(target_os = "macos")]
    let names = ["Menlo"];
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let names = [];
    let mut families: Vec<_> = names.into_iter().map(Family::Name).collect();
    families.push(Family::Monospace);
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    families.extend(["Noto Sans Mono", "DejaVu Sans Mono", "Liberation Mono"].map(Family::Name));
    families
}

fn resolve(database: &Database, families: &[Family<'_>]) -> Option<(String, Arc<FontData>)> {
    for family in families {
        let Some(id) = database.query(&Query {
            families: std::slice::from_ref(family),
            ..Default::default()
        }) else {
            continue;
        };
        let data = database
            .with_face_data(id, |bytes, index| {
                // Validate with the same parser egui uses before accepting an installed face.
                let font = FontRef::try_from_slice_and_index(bytes, index).ok()?;
                let latin = font.glyph_id('A');
                if latin.0 == 0 || font.outline(latin).is_none() {
                    // Bitmap emoji and symbol-only faces cannot serve as the UI text font.
                    return None;
                }
                Some(FontData {
                    index,
                    ..FontData::from_owned(bytes.to_vec())
                })
            })
            .flatten();
        if let Some(data) = data {
            return Some((format!("system-{id:?}"), Arc::new(data)));
        }
    }
    None
}

fn definitions(database: &Database) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        NOTO.into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/NotoSansSC-Regular.otf"
        ))),
    );
    let chinese = resolve(database, CHINESE);
    for (family, candidates) in [
        (FontFamily::Proportional, ui_families()),
        (FontFamily::Monospace, mono_families()),
    ] {
        let mut preferred = Vec::new();
        let primary = resolve(database, &candidates);
        let mut defaults = fonts.families.remove(&family).unwrap_or_default();
        if primary.is_none() && family == FontFamily::Monospace {
            // Keep Latin text monospaced even when only a system CJK face is available.
            preferred.append(&mut defaults);
        }
        for (name, data) in primary.into_iter().chain(chinese.clone()) {
            fonts.font_data.entry(name.clone()).or_insert(data);
            if !preferred.contains(&name) {
                preferred.push(name);
            }
        }
        preferred.push(NOTO.into());
        preferred.append(&mut defaults);
        fonts.families.insert(family, preferred);
    }
    fonts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_chinese_renders(fonts: FontDefinitions) {
        let context = egui::Context::default();
        context.set_fonts(fonts);
        let output =
            context.run(Default::default(), |context| {
                context.fonts_mut(|fonts| {
                for font in [egui::FontId::proportional(14.0), egui::FontId::monospace(13.0)] {
                    assert!(fonts.has_glyphs(&font, "简体中文繁體漢字压缩解压文件目录「」【】"));
                }
                let mono = egui::FontId::monospace(13.0);
                assert_eq!(fonts.glyph_width(&mono, 'i'), fonts.glyph_width(&mono, 'W'));
            });
                egui::CentralPanel::default().show(context, |ui| {
                    ui.label("中文目录 / 压缩文件.zip");
                });
            });
        assert!(!context
            .tessellate(output.shapes, output.pixels_per_point)
            .is_empty());
    }

    #[test]
    fn no_system_fonts_still_render_chinese_offline() {
        let fonts = definitions(&Database::new());
        assert_eq!(fonts.families[&FontFamily::Proportional][0], NOTO);
        assert_chinese_renders(fonts);
        let noto =
            FontRef::try_from_slice(include_bytes!("../assets/NotoSansSC-Regular.otf")).unwrap();
        // Ensure the OTF has drawable outlines, not just a character map.
        assert!(noto.outline(noto.glyph_id('中')).is_some());
    }

    #[test]
    fn latin_system_font_precedes_noto_and_missing_chinese_falls_back() {
        let mut database = Database::new();
        let defaults = FontDefinitions::default();
        database.load_font_data(defaults.font_data["Ubuntu-Light"].font.to_vec());
        let family = database.faces().next().unwrap().families[0].0.clone();
        database.set_sans_serif_family(family);
        let fonts = definitions(&database);
        let chain = &fonts.families[&FontFamily::Proportional];
        assert!(chain[0].starts_with("system-"));
        assert_eq!(chain[1], NOTO);
        let latin = &fonts.font_data[&chain[0]];
        let latin = FontRef::try_from_slice_and_index(&latin.font, latin.index).unwrap();
        assert_eq!(latin.glyph_id('中').0, 0);
        assert_chinese_renders(fonts);
    }

    #[test]
    fn unreadable_or_invalid_system_faces_are_skipped() {
        let mut source = Database::new();
        let defaults = FontDefinitions::default();
        let latin = defaults.font_data["Ubuntu-Light"].font.to_vec();
        source.load_font_data(latin.clone());
        let template = source.faces().next().unwrap().clone();
        let missing = tempfile::tempdir().unwrap().path().join("missing-font.ttf");
        for (data, index) in [
            (fontdb::Source::Binary(Arc::new(vec![0_u8; 16])), 0),
            (fontdb::Source::Binary(Arc::new(latin)), 99),
            (
                fontdb::Source::Binary(Arc::new(
                    defaults.font_data["NotoEmoji-Regular"].font.to_vec(),
                )),
                0,
            ),
            (fontdb::Source::File(missing), 0),
        ] {
            let mut database = Database::new();
            let mut face = template.clone();
            database.set_sans_serif_family(&face.families[0].0);
            face.source = data;
            face.index = index;
            database.push_face_info(face);
            let fonts = definitions(&database);
            assert_eq!(fonts.families[&FontFamily::Proportional][0], NOTO);
        }
    }

    #[test]
    fn selected_fonts_keep_chinese_fallbacks_and_missing_choices_restore_defaults() {
        let mut database = Database::new();
        let bundled = FontDefinitions::default();
        database.load_font_data(bundled.font_data["Ubuntu-Light"].font.to_vec());
        let family = database.faces().next().unwrap().families[0].0.clone();
        database.set_sans_serif_family(&family);
        let defaults = definitions(&database);
        for (selection, expected) in [
            (Selection::Noto, true),
            (Selection::Installed(family), true),
            (Selection::Installed("Uninstalled font".into()), false),
        ] {
            let (fonts, available) = selected_definitions(&database, &defaults, &selection);
            assert_eq!(available, expected);
            assert_eq!(
                fonts.families[&FontFamily::Monospace],
                defaults.families[&FontFamily::Monospace]
            );
            if selection == Selection::Noto {
                assert_eq!(fonts.families[&FontFamily::Proportional][0], NOTO);
            }
            if !available {
                assert_eq!(
                    fonts.families[&FontFamily::Proportional],
                    defaults.families[&FontFamily::Proportional]
                );
            }
            assert_chinese_renders(fonts);
        }
    }

    #[test]
    fn installed_chinese_font_precedes_embedded_noto() {
        let mut database = Database::new();
        database.load_font_data(include_bytes!("../assets/NotoSansSC-Regular.otf").to_vec());
        let fonts = definitions(&database);
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            let chain = &fonts.families[&family];
            let system = chain
                .iter()
                .position(|name| name.starts_with("system-"))
                .unwrap();
            let embedded = chain.iter().position(|name| name == NOTO).unwrap();
            assert!(system < embedded);
        }
        assert_chinese_renders(fonts);
    }
}
