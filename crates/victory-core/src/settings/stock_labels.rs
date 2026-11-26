//! English for the title's own Graphics rows, whose text this build does
//! not load (the rows show blank).
//!
//! Each row looks its name up by `joaat(key)` and its help by
//! `joaat(key + "_desc")`. The keys are our reconstruction, each confirmed by
//! its hash matching the row's ID; the wording is VIctory's own. All text
//! lives in this file, one table per language, so more languages can follow.

use super::text::{Text, joaat};

/// A stock row: its label key and VIctory's text for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StockLabel {
    /// The key the row's name ID is the hash of.
    pub key: &'static str,
    pub name: &'static str,
    /// Help text, for rows that have a help ID.
    pub help: Option<&'static str>,
}

impl StockLabel {
    /// The row's name and (if any) help, as menu text.
    pub fn texts(&self) -> impl Iterator<Item = Text> {
        let name = Text::title(joaat(self.key), self.name);
        let help = self.help.map(|help| Text::title(help_id(self.key), help));
        std::iter::once(name).chain(help)
    }
}

/// The help ID of the row named by `key`.
fn help_id(key: &str) -> u32 {
    joaat(&format!("{key}_desc"))
}

/// English. First a sample of rows to check the mapping in game; the rest
/// follow once it holds.
pub const EN: &[StockLabel] = &[
    StockLabel {
        key: "gfx_amblightingq",
        name: "Ambient lighting quality",
        help: Some("Quality of indirect light filling shaded areas."),
    },
    StockLabel {
        key: "gfx_fsr",
        name: "Upscaling quality",
        help: Some("Render resolution used by the upscaler. Lower is faster, higher is sharper."),
    },
    StockLabel {
        key: "gfx_hair_quality",
        name: "Hair quality",
        help: Some("Detail of character hair."),
    },
    StockLabel {
        key: "gfx_shader_taa",
        name: "Temporal anti-aliasing",
        help: Some("Smooths jagged edges by blending recent frames."),
    },
    StockLabel {
        key: "rtlodlightshadows",
        name: "Ray-traced distant light shadows",
        help: None,
    },
];

/// The table for a locale such as `en-US`; English when there is none.
pub fn for_locale(_locale: &str) -> &'static [StockLabel] {
    EN
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Row IDs as the title's rows hold them: the check that each key is
    /// the one the row hashes (name, help or 0 for none).
    const ROW_IDS: &[(&str, u32, u32)] = &[
        ("gfx_amblightingq", 0x6896_de76, 0x0651_dd7e),
        ("gfx_fsr", 0x0b25_dfcf, 0xda15_102a),
        ("gfx_hair_quality", 0x3e8c_994e, 0x5c7b_b3e9),
        ("gfx_shader_taa", 0x949a_6d1e, 0x9016_bdb5),
        ("rtlodlightshadows", 0x7eb7_28dd, 0),
    ];

    #[test]
    fn every_key_hashes_to_its_rows_ids() {
        for &(key, name, help) in ROW_IDS {
            let label = EN.iter().find(|l| l.key == key).expect("label for the row");
            let ids: Vec<u32> = label.texts().map(Text::id).collect();
            let want: Vec<u32> = [name, help].into_iter().filter(|&id| id != 0).collect();
            assert_eq!(ids, want, "{key}");
        }
    }

    #[test]
    fn no_text_is_empty_and_no_key_repeats() {
        let mut keys: Vec<&str> = EN.iter().map(|l| l.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), EN.len());
        assert!(
            EN.iter()
                .flat_map(StockLabel::texts)
                .all(|t| !t.english().is_empty())
        );
    }

    #[test]
    fn stock_ids_never_collide_with_our_own() {
        let ours = super::super::graphics::UPSCALING_GROUP.id();
        assert!(
            EN.iter()
                .flat_map(StockLabel::texts)
                .all(|t| t.id() != ours)
        );
    }
}
