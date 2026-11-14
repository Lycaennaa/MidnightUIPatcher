use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatchOptionKind {
    Color,
    Opacity,
    RelativeWidth,
    BorderWidth,
}

impl PatchOptionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Color => "color",
            Self::Opacity => "opacity",
            Self::RelativeWidth => "relative-width",
            Self::BorderWidth => "border-width",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PatchOption {
    MainPanelColor,
    MainPanelOpacity,
    MenuPopupColor,
    ButtonDefaultColor,
    ButtonIdleFill,
    InputTileBackgroundColor,
    InputTileIconOpacity,
    InputTileTextColor,
    InputTileBorderColor,
    InputTileBorderWidth,
    InputBannerTextColor,
    InputBannerMenuColor,
    InputBannerMenuFocusFillColor,
    InputBannerMenuFocusBorderColor,
    InputBannerMenuFocusBorderWidth,
    NoSignalTextColor,
}

impl PatchOption {
    pub(crate) fn canonical_id(self) -> &'static str {
        option_spec_by_option(self).id
    }

    pub(crate) fn kind(self) -> PatchOptionKind {
        option_spec_by_option(self).kind
    }

    pub(crate) fn qml_format_signature(self) -> String {
        option_spec_by_option(self).qml_format.signature()
    }
}

#[derive(Clone, Copy, Debug)]
enum PatchOptionQmlFormat {
    Direct,
    CurrentInputColor,
    CurrentInputWidth,
    NoSignalTextColor,
}

impl PatchOptionQmlFormat {
    const fn signature_id(self) -> &'static str {
        match self {
            Self::Direct => "direct:v1",
            Self::CurrentInputColor => "current-input-color:v1",
            Self::CurrentInputWidth => "current-input-width:v1",
            Self::NoSignalTextColor => "no-signal-text-color:v1",
        }
    }

    fn signature(self) -> String {
        format!(
            "{}:{}",
            self.signature_id(),
            self.apply("{VALUE}".to_owned())
        )
    }

    fn apply(self, value: String) -> String {
        match self {
            Self::Direct => value,
            Self::CurrentInputColor => {
                format!("entry.currentInput ? {value} : \"transparent\"")
            }
            Self::CurrentInputWidth => {
                format!("entry.currentInput ? styler.getRelativeValue({value}) : 0")
            }
            Self::NoSignalTextColor => format!(
                "(text === stringSheet.screensaver.strings.noSignal.full || text === stringSheet.screensaver.strings.noSignal.header) ? {value} : \"#E6E6E6\""
            ),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PatchOptionSpec {
    pub id: &'static str,
    pub patch_id: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub kind: PatchOptionKind,
    pub default: &'static str,
    option: PatchOption,
    qml_format: PatchOptionQmlFormat,
}

pub const PATCH_OPTION_SPECS: [PatchOptionSpec; 16] = [
    PatchOptionSpec {
        id: "main-panel-color",
        patch_id: "main-panel-color",
        label: "Panel backdrop color",
        help: "The Quick Settings panel background color.",
        kind: PatchOptionKind::Color,
        default: "#000000",
        option: PatchOption::MainPanelColor,
        qml_format: PatchOptionQmlFormat::Direct,
    },
    PatchOptionSpec {
        id: "main-panel-opacity",
        patch_id: "main-panel-opacity",
        label: "Panel backdrop opacity",
        help: "From 0 (transparent) to 1 (fully opaque).",
        kind: PatchOptionKind::Opacity,
        default: "1",
        option: PatchOption::MainPanelOpacity,
        qml_format: PatchOptionQmlFormat::Direct,
    },
    PatchOptionSpec {
        id: "menu-popup-color",
        patch_id: "menu-popup-color",
        label: "Menu popup color",
        help: "The menu popup background color.",
        kind: PatchOptionKind::Color,
        default: "#000000",
        option: PatchOption::MenuPopupColor,
        qml_format: PatchOptionQmlFormat::Direct,
    },
    PatchOptionSpec {
        id: "button-default-color",
        patch_id: "button-default-color",
        label: "Tile default fill",
        help: "The default Quick Settings tile fill color.",
        kind: PatchOptionKind::Color,
        default: "#000000",
        option: PatchOption::ButtonDefaultColor,
        qml_format: PatchOptionQmlFormat::Direct,
    },
    PatchOptionSpec {
        id: "button-idle-fill",
        patch_id: "button-idle-fill",
        label: "Tile idle fill",
        help: "Idle tile fill only; focus and disabled colors stay unchanged.",
        kind: PatchOptionKind::Color,
        default: "#000000",
        option: PatchOption::ButtonIdleFill,
        qml_format: PatchOptionQmlFormat::Direct,
    },
    PatchOptionSpec {
        id: "input-tile-background-color",
        patch_id: "input-tile-background",
        label: "Input tile background",
        help: "Defaults to transparent; choose an opaque or translucent fill if desired.",
        kind: PatchOptionKind::Color,
        default: "#00000000",
        option: PatchOption::InputTileBackgroundColor,
        qml_format: PatchOptionQmlFormat::Direct,
    },
    PatchOptionSpec {
        id: "input-tile-icon-opacity",
        patch_id: "input-tile-icons",
        label: "Input tile icon opacity",
        help: "From 0 (hidden) to 1 (fully opaque). The current-input marker remains hidden.",
        kind: PatchOptionKind::Opacity,
        default: "0",
        option: PatchOption::InputTileIconOpacity,
        qml_format: PatchOptionQmlFormat::Direct,
    },
    PatchOptionSpec {
        id: "input-tile-text-color",
        patch_id: "input-tile-red-text",
        label: "Input tile text color",
        help: "Color used for device and port labels.",
        kind: PatchOptionKind::Color,
        default: "#FF0000",
        option: PatchOption::InputTileTextColor,
        qml_format: PatchOptionQmlFormat::Direct,
    },
    PatchOptionSpec {
        id: "input-tile-border-color",
        patch_id: "input-tile-border",
        label: "Selected input border color",
        help: "The border color for the currently selected input; other inputs remain transparent.",
        kind: PatchOptionKind::Color,
        default: "#FFFFFFFF",
        option: PatchOption::InputTileBorderColor,
        qml_format: PatchOptionQmlFormat::CurrentInputColor,
    },
    PatchOptionSpec {
        id: "input-tile-border-width",
        patch_id: "input-tile-border",
        label: "Selected input border width",
        help: "Relative width from 0 to 100; non-selected inputs remain borderless.",
        kind: PatchOptionKind::RelativeWidth,
        default: "1",
        option: PatchOption::InputTileBorderWidth,
        qml_format: PatchOptionQmlFormat::CurrentInputWidth,
    },
    PatchOptionSpec {
        id: "input-banner-text-color",
        patch_id: "input-banner-text-color",
        label: "Input source banner text color",
        help: "Color used for the current source name and input label in the OK banner.",
        kind: PatchOptionKind::Color,
        default: "#FF0000",
        option: PatchOption::InputBannerTextColor,
        qml_format: PatchOptionQmlFormat::Direct,
    },
    PatchOptionSpec {
        id: "input-banner-menu-color",
        patch_id: "input-banner-menu-color",
        label: "Input banner menu icon color",
        help: "Color of the vertical three-dot menu in the OK banner.",
        kind: PatchOptionKind::Color,
        default: "#FF0000",
        option: PatchOption::InputBannerMenuColor,
        qml_format: PatchOptionQmlFormat::Direct,
    },
    PatchOptionSpec {
        id: "input-banner-menu-focus-fill-color",
        patch_id: "input-banner-menu-color",
        label: "Menu focused tile fill",
        help: "Background fill while the three-dot menu is focused; defaults to transparent.",
        kind: PatchOptionKind::Color,
        default: "#00000000",
        option: PatchOption::InputBannerMenuFocusFillColor,
        qml_format: PatchOptionQmlFormat::Direct,
    },
    PatchOptionSpec {
        id: "input-banner-menu-focus-border-color",
        patch_id: "input-banner-menu-color",
        label: "Menu focus border color",
        help: "Outline color while the three-dot menu is focused.",
        kind: PatchOptionKind::Color,
        default: "#E6E6E6",
        option: PatchOption::InputBannerMenuFocusBorderColor,
        qml_format: PatchOptionQmlFormat::Direct,
    },
    PatchOptionSpec {
        id: "input-banner-menu-focus-border-width",
        patch_id: "input-banner-menu-color",
        label: "Menu focus border width",
        help: "Outline width in QML units, from 0 to 10.",
        kind: PatchOptionKind::BorderWidth,
        default: "3",
        option: PatchOption::InputBannerMenuFocusBorderWidth,
        qml_format: PatchOptionQmlFormat::Direct,
    },
    PatchOptionSpec {
        id: "no-signal-text-color",
        patch_id: "no-signal-text-color",
        label: "No Signal text color",
        help: "Color of the translated No Signal title; other screensaver titles remain unchanged.",
        kind: PatchOptionKind::Color,
        default: "#FF0000",
        option: PatchOption::NoSignalTextColor,
        qml_format: PatchOptionQmlFormat::NoSignalTextColor,
    },
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PatchOptions {
    pub main_panel_color: String,
    pub main_panel_opacity: f64,
    pub menu_popup_color: String,
    pub button_default_color: String,
    pub button_idle_fill: String,
    pub input_tile_background_color: String,
    pub input_tile_icon_opacity: f64,
    pub input_tile_text_color: String,
    pub input_tile_border_color: String,
    pub input_tile_border_width: f64,
    pub input_banner_text_color: String,
    pub input_banner_menu_color: String,
    pub input_banner_menu_focus_fill_color: String,
    pub input_banner_menu_focus_border_color: String,
    pub input_banner_menu_focus_border_width: f64,
    pub no_signal_text_color: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PatchOptionError {
    #[error("unknown patch option `{0}`")]
    UnknownOption(String),
    #[error("patch option `{name}` must be #RRGGBB or Qt-style #AARRGGBB hex")]
    InvalidColor { name: &'static str },
    #[error("patch option `{name}` must be a finite number from {range}")]
    InvalidNumber {
        name: &'static str,
        range: &'static str,
    },
}

#[derive(Clone, Copy)]
enum PatchOptionValue<'a> {
    Color(&'a str),
    Number(f64),
}

enum ParsedPatchOptionValue {
    Color(String),
    Number(f64),
}

impl PatchOptionKind {
    fn numeric_range(self) -> (f64, f64, &'static str) {
        match self {
            Self::Opacity => (0.0, 1.0, "0 to 1"),
            Self::RelativeWidth => (0.0, 100.0, "0 to 100"),
            Self::BorderWidth => (0.0, 10.0, "0 to 10"),
            Self::Color => unreachable!("color options do not have numeric bounds"),
        }
    }

    fn parse(
        self,
        name: &'static str,
        value: &str,
    ) -> Result<ParsedPatchOptionValue, PatchOptionError> {
        match self {
            Self::Color => Ok(ParsedPatchOptionValue::Color(parse_color(name, value)?)),
            Self::Opacity | Self::RelativeWidth | Self::BorderWidth => {
                let (min, max, range) = self.numeric_range();
                Ok(ParsedPatchOptionValue::Number(parse_number(
                    name, value, min, max, range,
                )?))
            }
        }
    }

    fn validate(
        self,
        name: &'static str,
        value: PatchOptionValue<'_>,
    ) -> Result<(), PatchOptionError> {
        match (self, value) {
            (Self::Color, PatchOptionValue::Color(value)) => {
                if is_qt_hex_color(value) {
                    Ok(())
                } else {
                    Err(PatchOptionError::InvalidColor { name })
                }
            }
            (
                Self::Opacity | Self::RelativeWidth | Self::BorderWidth,
                PatchOptionValue::Number(value),
            ) => {
                let (min, max, range) = self.numeric_range();
                validate_number(name, value, min, max, range)
            }
            _ => unreachable!("patch option kind must match its stored value"),
        }
    }

    fn display(self, value: PatchOptionValue<'_>) -> String {
        match (self, value) {
            (Self::Color, PatchOptionValue::Color(value)) => value.to_owned(),
            (
                Self::Opacity | Self::RelativeWidth | Self::BorderWidth,
                PatchOptionValue::Number(value),
            ) => qml_number(value),
            _ => unreachable!("patch option kind must match its stored value"),
        }
    }

    fn qml_value(self, value: PatchOptionValue<'_>) -> String {
        match (self, value) {
            (Self::Color, PatchOptionValue::Color(value)) => qml_color(value),
            (
                Self::Opacity | Self::RelativeWidth | Self::BorderWidth,
                PatchOptionValue::Number(value),
            ) => qml_number(value),
            _ => unreachable!("patch option kind must match its stored value"),
        }
    }
}

impl Default for PatchOptions {
    fn default() -> Self {
        Self {
            main_panel_color: default_color(PatchOption::MainPanelColor),
            main_panel_opacity: default_number(PatchOption::MainPanelOpacity),
            menu_popup_color: default_color(PatchOption::MenuPopupColor),
            button_default_color: default_color(PatchOption::ButtonDefaultColor),
            button_idle_fill: default_color(PatchOption::ButtonIdleFill),
            input_tile_background_color: default_color(PatchOption::InputTileBackgroundColor),
            input_tile_icon_opacity: default_number(PatchOption::InputTileIconOpacity),
            input_tile_text_color: default_color(PatchOption::InputTileTextColor),
            input_tile_border_color: default_color(PatchOption::InputTileBorderColor),
            input_tile_border_width: default_number(PatchOption::InputTileBorderWidth),
            input_banner_text_color: default_color(PatchOption::InputBannerTextColor),
            input_banner_menu_color: default_color(PatchOption::InputBannerMenuColor),
            input_banner_menu_focus_fill_color: default_color(
                PatchOption::InputBannerMenuFocusFillColor,
            ),
            input_banner_menu_focus_border_color: default_color(
                PatchOption::InputBannerMenuFocusBorderColor,
            ),
            input_banner_menu_focus_border_width: default_number(
                PatchOption::InputBannerMenuFocusBorderWidth,
            ),
            no_signal_text_color: default_color(PatchOption::NoSignalTextColor),
        }
    }
}

impl PatchOptions {
    pub fn validate(&self) -> Result<(), PatchOptionError> {
        for spec in &PATCH_OPTION_SPECS {
            spec.kind
                .validate(spec.id, self.option_value(spec.option))?;
        }
        Ok(())
    }

    pub fn set_option(&mut self, id: &str, value: &str) -> Result<(), PatchOptionError> {
        let spec =
            option_spec_by_id(id).ok_or_else(|| PatchOptionError::UnknownOption(id.to_owned()))?;
        let parsed = spec.kind.parse(spec.id, value)?;
        self.set_option_value(spec.option, parsed);
        Ok(())
    }

    pub fn value(&self, id: &str) -> Option<String> {
        let spec = option_spec_by_id(id)?;
        Some(spec.kind.display(self.option_value(spec.option)))
    }

    pub(crate) fn option_replacement(&self, option: PatchOption) -> String {
        let spec = option_spec_by_option(option);
        let value = spec.kind.qml_value(self.option_value(option));
        spec.qml_format.apply(value)
    }

    fn option_value(&self, option: PatchOption) -> PatchOptionValue<'_> {
        match option {
            PatchOption::MainPanelColor => PatchOptionValue::Color(&self.main_panel_color),
            PatchOption::MainPanelOpacity => PatchOptionValue::Number(self.main_panel_opacity),
            PatchOption::MenuPopupColor => PatchOptionValue::Color(&self.menu_popup_color),
            PatchOption::ButtonDefaultColor => PatchOptionValue::Color(&self.button_default_color),
            PatchOption::ButtonIdleFill => PatchOptionValue::Color(&self.button_idle_fill),
            PatchOption::InputTileBackgroundColor => {
                PatchOptionValue::Color(&self.input_tile_background_color)
            }
            PatchOption::InputTileIconOpacity => {
                PatchOptionValue::Number(self.input_tile_icon_opacity)
            }
            PatchOption::InputTileTextColor => PatchOptionValue::Color(&self.input_tile_text_color),
            PatchOption::InputTileBorderColor => {
                PatchOptionValue::Color(&self.input_tile_border_color)
            }
            PatchOption::InputTileBorderWidth => {
                PatchOptionValue::Number(self.input_tile_border_width)
            }
            PatchOption::InputBannerTextColor => {
                PatchOptionValue::Color(&self.input_banner_text_color)
            }
            PatchOption::InputBannerMenuColor => {
                PatchOptionValue::Color(&self.input_banner_menu_color)
            }
            PatchOption::InputBannerMenuFocusFillColor => {
                PatchOptionValue::Color(&self.input_banner_menu_focus_fill_color)
            }
            PatchOption::InputBannerMenuFocusBorderColor => {
                PatchOptionValue::Color(&self.input_banner_menu_focus_border_color)
            }
            PatchOption::InputBannerMenuFocusBorderWidth => {
                PatchOptionValue::Number(self.input_banner_menu_focus_border_width)
            }
            PatchOption::NoSignalTextColor => PatchOptionValue::Color(&self.no_signal_text_color),
        }
    }

    fn set_option_value(&mut self, option: PatchOption, value: ParsedPatchOptionValue) {
        match (option, value) {
            (PatchOption::MainPanelColor, ParsedPatchOptionValue::Color(value)) => {
                self.main_panel_color = value
            }
            (PatchOption::MainPanelOpacity, ParsedPatchOptionValue::Number(value)) => {
                self.main_panel_opacity = value
            }
            (PatchOption::MenuPopupColor, ParsedPatchOptionValue::Color(value)) => {
                self.menu_popup_color = value
            }
            (PatchOption::ButtonDefaultColor, ParsedPatchOptionValue::Color(value)) => {
                self.button_default_color = value
            }
            (PatchOption::ButtonIdleFill, ParsedPatchOptionValue::Color(value)) => {
                self.button_idle_fill = value
            }
            (PatchOption::InputTileBackgroundColor, ParsedPatchOptionValue::Color(value)) => {
                self.input_tile_background_color = value
            }
            (PatchOption::InputTileIconOpacity, ParsedPatchOptionValue::Number(value)) => {
                self.input_tile_icon_opacity = value
            }
            (PatchOption::InputTileTextColor, ParsedPatchOptionValue::Color(value)) => {
                self.input_tile_text_color = value
            }
            (PatchOption::InputTileBorderColor, ParsedPatchOptionValue::Color(value)) => {
                self.input_tile_border_color = value
            }
            (PatchOption::InputTileBorderWidth, ParsedPatchOptionValue::Number(value)) => {
                self.input_tile_border_width = value
            }
            (PatchOption::InputBannerTextColor, ParsedPatchOptionValue::Color(value)) => {
                self.input_banner_text_color = value
            }
            (PatchOption::InputBannerMenuColor, ParsedPatchOptionValue::Color(value)) => {
                self.input_banner_menu_color = value
            }
            (PatchOption::InputBannerMenuFocusFillColor, ParsedPatchOptionValue::Color(value)) => {
                self.input_banner_menu_focus_fill_color = value
            }
            (
                PatchOption::InputBannerMenuFocusBorderColor,
                ParsedPatchOptionValue::Color(value),
            ) => self.input_banner_menu_focus_border_color = value,
            (
                PatchOption::InputBannerMenuFocusBorderWidth,
                ParsedPatchOptionValue::Number(value),
            ) => self.input_banner_menu_focus_border_width = value,
            (PatchOption::NoSignalTextColor, ParsedPatchOptionValue::Color(value)) => {
                self.no_signal_text_color = value
            }
            _ => unreachable!("patch option kind must match its stored field"),
        }
    }
}

fn option_spec_by_id(id: &str) -> Option<&'static PatchOptionSpec> {
    PATCH_OPTION_SPECS.iter().find(|spec| spec.id == id)
}

fn option_spec_by_option(option: PatchOption) -> &'static PatchOptionSpec {
    PATCH_OPTION_SPECS
        .iter()
        .find(|spec| spec.option == option)
        .expect("every patch option must have a specification")
}

fn default_option_value(option: PatchOption) -> ParsedPatchOptionValue {
    let spec = option_spec_by_option(option);
    spec.kind
        .parse(spec.id, spec.default)
        .expect("patch option defaults must be valid")
}

fn default_color(option: PatchOption) -> String {
    match default_option_value(option) {
        ParsedPatchOptionValue::Color(value) => value,
        ParsedPatchOptionValue::Number(_) => unreachable!("color option must have a color default"),
    }
}

fn default_number(option: PatchOption) -> f64 {
    match default_option_value(option) {
        ParsedPatchOptionValue::Number(value) => value,
        ParsedPatchOptionValue::Color(_) => {
            unreachable!("numeric option must have a numeric default")
        }
    }
}

fn is_qt_hex_color(value: &str) -> bool {
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };
    matches!(hex.len(), 6 | 8) && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn parse_color(name: &'static str, value: &str) -> Result<String, PatchOptionError> {
    if !is_qt_hex_color(value) {
        return Err(PatchOptionError::InvalidColor { name });
    }
    Ok(value.to_ascii_uppercase())
}

fn parse_number(
    name: &'static str,
    value: &str,
    min: f64,
    max: f64,
    range: &'static str,
) -> Result<f64, PatchOptionError> {
    let number = value
        .parse::<f64>()
        .map_err(|_| PatchOptionError::InvalidNumber { name, range })?;
    validate_number(name, number, min, max, range)?;
    Ok(number)
}

fn validate_number(
    name: &'static str,
    value: f64,
    min: f64,
    max: f64,
    range: &'static str,
) -> Result<(), PatchOptionError> {
    if value.is_finite() && (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(PatchOptionError::InvalidNumber { name, range })
    }
}

fn qml_color(color: &str) -> String {
    format!("\"{color}\"")
}

fn qml_number(value: f64) -> String {
    if value == 0.0 {
        "0".to_owned()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_option_canonical_ids_match_specs() {
        for spec in PATCH_OPTION_SPECS {
            assert_eq!(spec.option.canonical_id(), spec.id);
            assert!(!spec.option.qml_format_signature().is_empty());
        }
    }
}
