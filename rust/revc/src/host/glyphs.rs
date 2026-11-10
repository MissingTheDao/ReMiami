//! Controller button glyphs for in-text prompts (plan task 9).
//!
//! The game's prompt text uses positional tokens (`~X~` = bottom face button, `~K~` = left
//! shoulder ...; CFont, BUTTON_ICONS) drawn with CFont::ButtonSprite. These come from the Kenney
//! Input Prompts pack in `assets/glyphs/kenney-input-prompts` (CC0, see its SOURCE.md), picked for
//! the pad the player used last. Face buttons use the label SDL reports for the pad (so a Switch
//! pad's bottom button shows "B" and a DualSense's shows the cross).

use super::pads::PadKind;
use sdl3_sys::everything::*;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// CFont's BUTTON_* order (renderer/Font.h).
pub const BUTTON_COUNT: usize = 24;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Button {
    LStickUp = 0,
    LStickDown,
    LStickLeft,
    LStickRight,
    South, // BUTTON_CROSS
    East,  // BUTTON_CIRCLE
    West,  // BUTTON_SQUARE
    North, // BUTTON_TRIANGLE
    L1,
    L2,
    L3,
    R1,
    R2,
    R3,
    RStickUp,
    RStickDown,
    RStickLeft,
    RStickRight,
    DpadUp,
    DpadRight,
    DpadDown,
    DpadLeft,
    Select, // Back / View / Share / Create / Minus
    Start,  // Start / Menu / Options / Plus
}

const ALL: [Button; BUTTON_COUNT] = [
    Button::LStickUp,
    Button::LStickDown,
    Button::LStickLeft,
    Button::LStickRight,
    Button::South,
    Button::East,
    Button::West,
    Button::North,
    Button::L1,
    Button::L2,
    Button::L3,
    Button::R1,
    Button::R2,
    Button::R3,
    Button::RStickUp,
    Button::RStickDown,
    Button::RStickLeft,
    Button::RStickRight,
    Button::DpadUp,
    Button::DpadRight,
    Button::DpadDown,
    Button::DpadLeft,
    Button::Select,
    Button::Start,
];

/// Face-button labels of a pad in South, East, West, North order (SDL_GetGamepadButtonLabel).
pub type FaceLabels = [SDL_GamepadButtonLabel; 4];

/// Labels SDL uses for an Xbox-style pad, the default when a pad reports none.
pub const XBOX_LABELS: FaceLabels =
    [SDL_GAMEPAD_BUTTON_LABEL_A, SDL_GAMEPAD_BUTTON_LABEL_B, SDL_GAMEPAD_BUTTON_LABEL_X, SDL_GAMEPAD_BUTTON_LABEL_Y];

/// Labels of the styles the Controls menu can force (and of real pads of those families).
pub const PS_LABELS: FaceLabels = [
    SDL_GAMEPAD_BUTTON_LABEL_CROSS,
    SDL_GAMEPAD_BUTTON_LABEL_CIRCLE,
    SDL_GAMEPAD_BUTTON_LABEL_SQUARE,
    SDL_GAMEPAD_BUTTON_LABEL_TRIANGLE,
];
/// Nintendo layout: bottom B, right A, left Y, top X.
pub const NINTENDO_LABELS: FaceLabels =
    [SDL_GAMEPAD_BUTTON_LABEL_B, SDL_GAMEPAD_BUTTON_LABEL_A, SDL_GAMEPAD_BUTTON_LABEL_Y, SDL_GAMEPAD_BUTTON_LABEL_X];

/// Style forced in the Controls menu ("BUTTON PROMPTS"): 1 Xbox, 2 PlayStation, 3 Nintendo,
/// 4 Steam Deck, 5 Steam Controller; anything else follows the pad in use (None).
pub fn override_style(style: i32) -> Option<(PadKind, FaceLabels)> {
    Some(match style {
        1 => (PadKind::XboxSeries, XBOX_LABELS),
        2 => (PadKind::PlayStation5, PS_LABELS),
        3 => (PadKind::Switch, NINTENDO_LABELS),
        4 => (PadKind::SteamDeck, XBOX_LABELS),
        5 => (PadKind::SteamController, XBOX_LABELS),
        _ => return None,
    })
}

/// Glyph file (folder inside the pack, file name in its Double/ folder) for one button.
pub fn glyph_file(kind: PadKind, labels: FaceLabels, b: Button) -> Option<(&'static str, String)> {
    use Button::*;
    let (folder, p) = match kind {
        PadKind::KeyboardMouse => return None,
        PadKind::PlayStation3 | PadKind::PlayStation4 | PadKind::PlayStation5 => ("PlayStation Series", "playstation"),
        PadKind::Switch => ("Nintendo Switch", "switch"),
        PadKind::GameCube => ("Nintendo Gamecube", "gamecube"),
        PadKind::SteamDeck => ("Steam Deck", "steamdeck"),
        PadKind::SteamController => ("Steam Controller", "steam"),
        PadKind::Xbox360 | PadKind::XboxSeries | PadKind::Generic => ("Xbox Series", "xbox"),
    };
    let face = |i: usize| -> String {
        let label = match labels[i] {
            SDL_GAMEPAD_BUTTON_LABEL_A => "a",
            SDL_GAMEPAD_BUTTON_LABEL_B => "b",
            SDL_GAMEPAD_BUTTON_LABEL_X => "x",
            SDL_GAMEPAD_BUTTON_LABEL_Y => "y",
            SDL_GAMEPAD_BUTTON_LABEL_CROSS => "cross",
            SDL_GAMEPAD_BUTTON_LABEL_CIRCLE => "circle",
            SDL_GAMEPAD_BUTTON_LABEL_SQUARE => "square",
            SDL_GAMEPAD_BUTTON_LABEL_TRIANGLE => "triangle",
            _ => ["a", "b", "x", "y"][i],
        };
        format!("{p}_button_{label}")
    };
    let stick = |side: char, dir: &str| -> String {
        match (kind, side) {
            // One stick; the right "stick" is the right trackpad.
            (PadKind::SteamController, 'l') => format!("steam_stick_{dir}"),
            (PadKind::SteamController, _) => format!("steam_pad_{dir}"),
            (PadKind::GameCube, 'l') => format!("gamecube_stick_{dir}"),
            (PadKind::GameCube, _) => format!("gamecube_stick_c_{dir}"),
            _ => format!("{p}_stick_{side}_{dir}"),
        }
    };
    let name = match b {
        LStickUp => stick('l', "up"),
        LStickDown => stick('l', "down"),
        LStickLeft => stick('l', "left"),
        LStickRight => stick('l', "right"),
        RStickUp => stick('r', "up"),
        RStickDown => stick('r', "down"),
        RStickLeft => stick('r', "left"),
        RStickRight => stick('r', "right"),
        South => face(0),
        East => face(1),
        West => face(2),
        North => face(3),
        L1 | L2 | R1 | R2 => {
            let (l, one) = (matches!(b, L1 | L2), matches!(b, L1 | R1));
            match kind {
                PadKind::PlayStation3 | PadKind::PlayStation4 | PadKind::PlayStation5 => {
                    format!("playstation_trigger_{}{}", if l { 'l' } else { 'r' }, if one { 1 } else { 2 })
                }
                PadKind::Switch => format!("switch_button_{}{}", if one { "" } else { "z" }, if l { 'l' } else { 'r' }),
                PadKind::SteamDeck => format!("steamdeck_button_{}{}", if l { 'l' } else { 'r' }, if one { 1 } else { 2 }),
                // SDL maps the GameCube's Z to the right shoulder; it has no left one.
                PadKind::GameCube => match b {
                    R1 => "gamecube_button_z".into(),
                    R2 => "gamecube_trigger_r".into(),
                    _ => "gamecube_trigger_l".into(),
                },
                _ => format!("{p}_{}{}", if l { 'l' } else { 'r' }, if one { 'b' } else { 't' }),
            }
        }
        DpadUp => format!("{p}_dpad_up"),
        DpadRight => format!("{p}_dpad_right"),
        DpadDown => format!("{p}_dpad_down"),
        DpadLeft => format!("{p}_dpad_left"),
        Select => match kind {
            PadKind::Xbox360 => "xbox_button_back".into(),
            PadKind::PlayStation3 => "playstation3_button_select".into(),
            PadKind::PlayStation4 => "playstation4_button_share".into(),
            PadKind::PlayStation5 => "playstation5_button_create".into(),
            PadKind::Switch => "switch_button_minus".into(),
            PadKind::GameCube => return None, // no such button
            PadKind::SteamDeck => "steamdeck_button_view".into(),
            PadKind::SteamController => "steam_button_back_icon".into(),
            _ => "xbox_button_view".into(),
        },
        Start => match kind {
            PadKind::Xbox360 => "xbox_button_start".into(),
            PadKind::PlayStation3 => "playstation3_button_start".into(),
            PadKind::PlayStation4 => "playstation4_button_options".into(),
            PadKind::PlayStation5 => "playstation5_button_options".into(),
            PadKind::Switch => "switch_button_plus".into(),
            PadKind::GameCube => "gamecube_button_start".into(),
            PadKind::SteamDeck => "steamdeck_button_options".into(),
            PadKind::SteamController => "steam_button_start_icon".into(),
            _ => "xbox_button_menu".into(),
        },
        L3 | R3 => match kind {
            PadKind::SteamController if b == L3 => "steam_stick_l_press".into(),
            PadKind::SteamController => "steam_pad_center".into(),
            PadKind::GameCube if b == L3 => "gamecube_stick".into(),
            PadKind::GameCube => "gamecube_stick_c".into(),
            _ => format!("{p}_stick_{}_press", if b == L3 { 'l' } else { 'r' }),
        },
    };
    Some((folder, format!("{name}.png")))
}

/// `assets/` of this build: $REVC_ASSETS_DIR, next to the executable, or the repository checkout
/// the executable was built in (rust/target/<profile>/revc -> ../../../assets).
pub fn assets_dir() -> Option<&'static Path> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        let marker = |d: &Path| d.join("glyphs/kenney-input-prompts/SOURCE.md").is_file();
        let mut candidates: Vec<PathBuf> = std::env::var_os("REVC_ASSETS_DIR").map(PathBuf::from).into_iter().collect();
        if let Ok(exe) = std::env::current_exe() {
            let exe = exe.canonicalize().unwrap_or(exe);
            for up in exe.ancestors().skip(1).take(5) {
                candidates.push(up.join("assets"));
            }
        }
        let found = candidates.into_iter().find(|d| marker(d));
        match &found {
            Some(d) => eprintln!("glyphs: using {}", d.display()),
            None => eprintln!("glyphs: assets/glyphs not found (set REVC_ASSETS_DIR); using the game's button TXDs"),
        }
        found
    })
    .as_deref()
}

/// Path of the glyph for `b` on a pad of this kind and layout, if the pack has it.
pub fn glyph_path(kind: PadKind, labels: FaceLabels, b: Button) -> Option<PathBuf> {
    let (folder, file) = glyph_file(kind, labels, b)?;
    let p = assets_dir()?.join("glyphs/kenney-input-prompts").join(folder).join("Double").join(file);
    p.is_file().then_some(p)
}

pub fn button(index: usize) -> Option<Button> {
    ALL.get(index).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(kind: PadKind, labels: FaceLabels, b: Button) -> String {
        glyph_file(kind, labels, b).unwrap().1
    }

    const SWITCH_LABELS: FaceLabels = NINTENDO_LABELS;

    #[test]
    fn menu_override_styles() {
        assert!(override_style(0).is_none());
        assert_eq!(override_style(2).unwrap().0, PadKind::PlayStation5);
        assert_eq!(file(PadKind::Switch, override_style(3).unwrap().1, Button::East), "switch_button_a.png");
        assert!(override_style(9).is_none());
    }

    #[test]
    fn maps_buttons_per_pad() {
        assert_eq!(file(PadKind::XboxSeries, XBOX_LABELS, Button::South), "xbox_button_a.png");
        assert_eq!(file(PadKind::XboxSeries, XBOX_LABELS, Button::L2), "xbox_lt.png");
        assert_eq!(file(PadKind::XboxSeries, XBOX_LABELS, Button::R3), "xbox_stick_r_press.png");
        assert_eq!(file(PadKind::PlayStation5, PS_LABELS, Button::South), "playstation_button_cross.png");
        assert_eq!(file(PadKind::PlayStation5, PS_LABELS, Button::R1), "playstation_trigger_r1.png");
        assert_eq!(file(PadKind::Switch, SWITCH_LABELS, Button::South), "switch_button_b.png");
        assert_eq!(file(PadKind::Switch, SWITCH_LABELS, Button::L2), "switch_button_zl.png");
        assert_eq!(file(PadKind::SteamDeck, XBOX_LABELS, Button::L1), "steamdeck_button_l1.png");
        assert_eq!(file(PadKind::SteamController, XBOX_LABELS, Button::RStickUp), "steam_pad_up.png");
        assert_eq!(file(PadKind::GameCube, XBOX_LABELS, Button::R1), "gamecube_button_z.png");
        assert!(glyph_file(PadKind::KeyboardMouse, XBOX_LABELS, Button::South).is_none());
    }

    /// Every glyph the mapping names exists in the committed pack.
    #[test]
    fn all_files_exist() {
        let pack = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/glyphs/kenney-input-prompts");
        let kinds = [
            (PadKind::XboxSeries, XBOX_LABELS),
            (PadKind::Xbox360, XBOX_LABELS),
            (PadKind::PlayStation4, PS_LABELS),
            (PadKind::PlayStation5, PS_LABELS),
            (PadKind::Switch, SWITCH_LABELS),
            (PadKind::GameCube, XBOX_LABELS),
            (PadKind::SteamDeck, XBOX_LABELS),
            (PadKind::SteamController, XBOX_LABELS),
            (PadKind::Generic, XBOX_LABELS),
        ];
        let mut missing = Vec::new();
        for (kind, labels) in kinds {
            for b in ALL {
                let (folder, f) = glyph_file(kind, labels, b).unwrap();
                if !pack.join(folder).join("Double").join(&f).is_file() {
                    missing.push(format!("{kind:?} {b:?}: {folder}/{f}"));
                }
            }
        }
        assert!(missing.is_empty(), "missing glyphs:\n{}", missing.join("\n"));
    }
}
