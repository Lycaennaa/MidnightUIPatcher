use super::{PATCHES, PatchArea};

fn ids_for(area: PatchArea) -> Vec<&'static str> {
    PATCHES
        .iter()
        .filter(|patch| patch.area == area)
        .map(|patch| patch.id)
        .collect()
}

#[test]
fn tweaks_are_grouped_by_affected_area() {
    assert_eq!(
        ids_for(PatchArea::QuickSettings),
        vec![
            "main-panel-color",
            "main-panel-opacity",
            "main-panel-visibility",
            "menu-popup-color",
            "button-default-color",
            "button-idle-fill",
        ],
    );
    assert_eq!(
        ids_for(PatchArea::InputPicker),
        vec![
            "input-screen-backdrop",
            "input-tile-shadow",
            "input-home-hub",
            "input-live-tv",
            "input-active-signal",
            "input-category-text",
            "input-universal-control",
            "input-tile-background",
            "input-tile-icons",
            "input-tile-red-text",
            "input-tile-border",
        ],
    );
    assert_eq!(
        ids_for(PatchArea::InputBanner),
        vec![
            "input-banner-background",
            "input-banner-text-color",
            "input-banner-divider",
            "input-banner-menu-color",
        ],
    );
}
