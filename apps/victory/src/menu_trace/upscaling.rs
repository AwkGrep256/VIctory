//! Prototype the native Upscaling group and an Upscaler presentation row.
//! The group and row use VIctory's text IDs; a native text provider is still
//! needed to make them visible. The probe registers no selection action.

use victory_loader::LoadedImage;

use crate::title_pattern::find_once;

// Native helpers the title itself uses for programmatically added rows.
const GROUP_ENTRY: &str = "56 57 53 48 83 EC 20 48 85 D2 74 0B 48 89 D0 8B 12 \
                           44 8B 40 04 EB 05 31 D2 45 31 C0 4C 8B 89 20 02 00 00";
const BUTTON_ENTRY: &str = "41 57 41 56 56 57 55 53 48 83 EC 28 48 89 CE 48 8B 0D ?? ?? ?? ?? \
                            44 8B BC 24 80 00 00 00 44 89 C7 44 89 CD 89 D3";

// joaat of VIctory-owned names, kept apart from stock labels and settings.
const GROUP_LABEL: u32 = 0xf53d_c350;
const UPSCALER_LABEL: u32 = 0xf117_5af5;
const UPSCALER_HELP: u32 = 0xdcf5_8a54;

#[repr(C)]
struct GroupKey {
    label: u32,
    order: u32,
}

type GetGroup = unsafe extern "win64" fn(usize, *const GroupKey) -> usize;
type AddButton = unsafe extern "win64" fn(usize, u32, u32, u32, u32);

pub(super) struct UpscalingProbe {
    group: GetGroup,
    button: AddButton,
}

impl UpscalingProbe {
    pub(super) fn locate(image: &LoadedImage) -> Option<Self> {
        if !victory_gpu::switch::on("VICTORY_MENU_UPSCALING_PROBE") {
            return None;
        }
        let group = find_once(image, "native settings group", GROUP_ENTRY)?;
        let button = find_once(image, "native settings button", BUTTON_ENTRY)?;
        // SAFETY: both entries and their native calling conventions were
        // checked. The mapped title lives for the duration of the probe.
        Some(unsafe {
            Self {
                group: std::mem::transmute::<usize, GetGroup>(image.base() + group),
                button: std::mem::transmute::<usize, AddButton>(image.base() + button),
            }
        })
    }

    /// Appends one group to the live Settings contents. `owner` must be the
    /// owner in the Graphics row visitor's context, on the menu thread.
    pub(super) unsafe fn append(&self, owner: usize) {
        let key = GroupKey {
            label: GROUP_LABEL,
            order: 255,
        };
        // SAFETY: the caller supplies the live menu owner on its own thread;
        // the title reads the key immediately and owns the returned group.
        let group = unsafe { (self.group)(owner, &key) };
        if group == 0 {
            return;
        }
        // SAFETY: the native helper owns the allocated row and appends it to
        // this group. The action ID is empty and carries no stock setting.
        unsafe { (self.button)(group, 0, 0, UPSCALER_LABEL, UPSCALER_HELP) };
        tracing::info!(target: "menu_trace", "appended the native Upscaling group and Upscaler probe row");
    }
}
