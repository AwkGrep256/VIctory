//! VIctory's Upscaling section in Settings → Graphics: a heading and one
//! native ◀ ▶ row per entry of [`ROWS`], with VIctory's own text.
//!
//! The title's group helper makes the heading and its button helper appends
//! a pooled row, which [`NativeList::adopt`] turns into a list row for the
//! setting. What each setting is lives in `victory_core::settings`.

use victory_core::settings::Text;
use victory_core::settings::graphics::{GRAPHICS_TAB, UPSCALING_GROUP};
use victory_loader::LoadedImage;

use super::native_list::NativeList;
use super::settings_list::{ALL, ListSetting};
use crate::title_pattern::find_once;

/// The rows of the section, in order: every list setting.
const ROWS: &[&ListSetting] = &ALL;

// Native helpers the title itself uses for programmatically added rows.
const GROUP_ENTRY: &str = "56 57 53 48 83 EC 20 48 85 D2 74 0B 48 89 D0 8B 12 \
                           44 8B 40 04 EB 05 31 D2 45 31 C0 4C 8B 89 20 02 00 00";
const BUTTON_ENTRY: &str = "41 57 41 56 56 57 55 53 48 83 EC 28 48 89 CE 48 8B 0D ?? ?? ?? ?? \
                            44 8B BC 24 80 00 00 00 44 89 C7 44 89 CD 89 D3";

/// Where the section sorts among the tab's groups.
const GROUP_ORDER: u32 = 255;

#[repr(C)]
struct GroupKey {
    label: u32,
    order: u32,
}

type GetGroup = unsafe extern "win64" fn(usize, *const GroupKey) -> usize;
type AddButton = unsafe extern "win64" fn(usize, u32, u32, u32, u32);

pub(super) struct UpscalingSection {
    group: GetGroup,
    button: AddButton,
    list: &'static NativeList,
}

impl UpscalingSection {
    pub(super) fn locate(image: &LoadedImage) -> Option<Self> {
        if !victory_gpu::switch::on("VICTORY_MENU_UPSCALING_PROBE") {
            return None;
        }
        let group = find_once(image, "native settings group", GROUP_ENTRY)?;
        let button = find_once(image, "native settings button", BUTTON_ENTRY)?;
        let list = NativeList::locate(image)?;
        // Before Settings first opens: its text map is noticed as it loads.
        super::menu_text::watch();
        // SAFETY: both entries and their native calling conventions were
        // checked. The mapped title lives for the duration of the process.
        Some(unsafe {
            Self {
                group: std::mem::transmute::<usize, GetGroup>(image.base() + group),
                button: std::mem::transmute::<usize, AddButton>(image.base() + button),
                list,
            }
        })
    }

    /// Every text the section shows, for the menu's text table.
    pub(super) fn texts() -> Vec<Text> {
        let mut texts = vec![GRAPHICS_TAB, UPSCALING_GROUP];
        texts.extend(ROWS.iter().flat_map(|s| [s.name(), s.help()]));
        // The stock Graphics rows' names, blank in this build otherwise.
        let locale = std::env::var("VICTORY_LOCALE").unwrap_or_default();
        texts.extend(
            victory_core::settings::stock_labels::for_locale(&locale)
                .iter()
                .flat_map(|label| label.texts()),
        );
        texts.extend(
            victory_core::settings::stock_labels::unnamed_for_locale(&locale)
                .iter()
                .flat_map(|row| [row.name, row.help]),
        );
        texts
    }

    /// Adds the section's text to the menu's text table. Cheap once in.
    pub(super) fn provide_text(&self, image_base: usize) {
        let texts: Vec<(u32, &'static str)> = Self::texts()
            .iter()
            .map(|t| (t.id(), t.english()))
            .collect();
        super::menu_text::provide(image_base, &texts);
    }

    /// Appends the section to the live Settings contents.
    ///
    /// # Safety
    /// `owner` is the owner in the Graphics row visitor's context, on the
    /// menu thread, after the stock rows were built.
    pub(super) unsafe fn append(&self, owner: usize) {
        let key = GroupKey {
            label: UPSCALING_GROUP.id(),
            order: GROUP_ORDER,
        };
        // SAFETY: the caller's live menu owner on its own thread; the title
        // reads the key at once and owns the returned group.
        let group = unsafe { (self.group)(owner, &key) };
        if group == 0 {
            return;
        }
        // Diagnostics, once: every group's heading ID, to give the stock
        // headings text.
        static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !LOGGED.swap(true, std::sync::atomic::Ordering::AcqRel) {
            log_group_headings(owner, group);
        }
        for setting in ROWS {
            // SAFETY: the helper allocates a pooled row and appends it to
            // this group; the action ID is empty and runs nothing.
            // SAFETY: a live group starts with {cap: u32, count: u32, rows}.
            let before = unsafe { std::ptr::read_unaligned((group + 4) as *const u32) };
            // SAFETY: as above; the helper appends a pooled row and runs nothing.
            unsafe { (self.button)(group, 0, 0, setting.name().id(), setting.help().id()) };
            // SAFETY: as above.
            let after = unsafe { std::ptr::read_unaligned((group + 4) as *const u32) };
            if after != before.wrapping_add(1) {
                tracing::warn!(target: "menu_trace", before, after, "row not appended, not adopted");
                continue;
            }
            // SAFETY: a live group whose last row was just appended.
            if let Some(row) = unsafe { last_row(group) }.filter(|&row| row != 0) {
                // SAFETY: a pooled settings row, just appended, not shown.
                unsafe { self.list.adopt(row, setting) };
            }
        }
        tracing::info!(target: "menu_trace", rows = ROWS.len(), "appended VIctory's Upscaling section");
    }
}

/// Diagnostics: finds the owner's group array (a pointer in the owner that
/// `group` lies in, at a whole number of strides) and logs each group's
/// heading ID (`+0x10`) and order (`+0x14`), as seen for VIctory's own.
fn log_group_headings(owner: usize, group: usize) {
    use victory_loader::read_memory;
    let u32_at = |at: usize| read_memory(at, 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    let Some(words) = read_memory(owner, 0x300) else {
        return;
    };
    for (i, word) in words.as_chunks::<8>().0.iter().enumerate() {
        let start = usize::from_le_bytes(*word);
        let Some(delta) = group.checked_sub(start).filter(|&d| d < 0x800) else {
            continue;
        };
        for stride in (0x30..=0x60).step_by(8) {
            if delta % stride != 0 {
                continue;
            }
            let mut headings = Vec::new();
            for n in 0..24 {
                let at = start + n * stride;
                let (Some(cap), Some(count), Some(label), Some(order)) = (
                    u32_at(at),
                    u32_at(at + 4),
                    u32_at(at + 0x10),
                    u32_at(at + 0x14),
                ) else {
                    break;
                };
                if label == 0 || order > 0xffff || cap > 4096 || count > cap {
                    break;
                }
                headings.push(format!("{label:08x}/{order}"));
            }
            if headings.len() > delta / stride {
                tracing::info!(target: "menu_trace", owner_offset = %format_args!("{:#x}", i * 8),
                               stride = %format_args!("{stride:#x}"), groups = ?headings,
                               "settings group headings");
            }
        }
    }
}

/// The last row of `group`'s row list.
///
/// # Safety
/// `group` is a live settings group.
unsafe fn last_row(group: usize) -> Option<usize> {
    // SAFETY: the caller's guarantee; a group starts with its row list
    // {cap: u32, count: u32, rows: *const usize}.
    unsafe {
        let count = std::ptr::read_unaligned((group + 4) as *const u32) as usize;
        let rows = std::ptr::read_unaligned((group + 8) as *const usize);
        (count != 0 && rows != 0)
            .then(|| std::ptr::read_unaligned((rows + (count - 1) * 8) as *const usize))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_section_supplies_every_text_it_shows() {
        let ids: Vec<u32> = UpscalingSection::texts().iter().map(|t| t.id()).collect();
        assert!(ids.contains(&GRAPHICS_TAB.id()));
        assert!(ids.contains(&UPSCALING_GROUP.id()));
        for setting in ROWS {
            assert!(ids.contains(&setting.name().id()) && ids.contains(&setting.help().id()));
        }
    }

    #[test]
    fn no_text_is_empty() {
        assert!(
            UpscalingSection::texts()
                .iter()
                .all(|t| !t.english().is_empty())
        );
    }
}
