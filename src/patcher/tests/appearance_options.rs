use super::*;
#[test]
fn appearance_numeric_options_reject_values_outside_their_ranges() {
    let mut options = PatchOptions::default();
    options.set_option("main-panel-opacity", "0").unwrap();
    options.set_option("input-tile-icon-opacity", "1").unwrap();
    options
        .set_option("input-tile-border-width", "100")
        .unwrap();
    options
        .set_option("input-banner-menu-focus-border-width", "10")
        .unwrap();
    options.validate().unwrap();

    for (id, value, name) in [
        ("main-panel-opacity", "1.01", "main-panel-opacity"),
        ("input-tile-icon-opacity", "NaN", "input-tile-icon-opacity"),
        ("input-tile-border-width", "-0.1", "input-tile-border-width"),
        (
            "input-tile-border-width",
            "100.1",
            "input-tile-border-width",
        ),
        (
            "input-banner-menu-focus-border-width",
            "10.1",
            "input-banner-menu-focus-border-width",
        ),
    ] {
        assert!(matches!(
            options.set_option(id, value),
            Err(crate::patches::PatchOptionError::InvalidNumber {
                name: actual,
                ..
            }) if actual == name
        ));
    }
}

#[test]
fn option_specs_are_authoritative_for_defaults_and_value_access() {
    let mut options = PatchOptions::default();
    for spec in &PATCH_OPTION_SPECS {
        assert_eq!(
            options.value(spec.id).as_deref(),
            Some(spec.default),
            "default for `{}` must come from its option specification",
            spec.id
        );
        options.set_option(spec.id, spec.default).unwrap();
    }
    options.validate().unwrap();
}
