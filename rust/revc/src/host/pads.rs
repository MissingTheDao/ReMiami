//! Which kind of controller a pad is, for button glyphs (plan task 9, docs/CONTROLLER_GLYPHS.md).
//!
//! The classification uses SDL3's gamepad type first (it reflects the physical pad even behind
//! Steam Input when Steam exposes it) and the USB vendor/product ids for Valve hardware that SDL
//! reports as a generic type.

use sdl3_sys::everything::*;

/// Glyph set to show. The numeric values are part of the C API (`hostActiveInputKind`).
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadKind {
    KeyboardMouse = 0,
    /// Joystick without a gamepad mapping: numbered buttons.
    Generic = 1,
    Xbox360 = 2,
    XboxSeries = 3,
    PlayStation3 = 4,
    PlayStation4 = 5,
    PlayStation5 = 6,
    Switch = 7,
    GameCube = 8,
    SteamDeck = 9,
    SteamController = 10,
}

impl PadKind {
    pub fn from_i32(v: i32) -> Option<PadKind> {
        use PadKind::*;
        [KeyboardMouse, Generic, Xbox360, XboxSeries, PlayStation3, PlayStation4, PlayStation5, Switch, GameCube, SteamDeck, SteamController]
            .into_iter()
            .find(|k| *k as i32 == v)
    }
}

pub const VALVE_VENDOR: u16 = 0x28DE;
pub const STEAM_DECK_PRODUCT: u16 = 0x1205;
/// Original Steam Controller: wired, wireless dongle, Bluetooth LE.
pub const STEAM_CONTROLLER_PRODUCTS: [u16; 3] = [0x1102, 0x1142, 0x1106];

/// `is_gamepad`: SDL has a gamepad mapping for the device (otherwise it's a raw joystick).
pub fn classify(is_gamepad: bool, ty: SDL_GamepadType, vendor: u16, product: u16) -> PadKind {
    if vendor == VALVE_VENDOR && product == STEAM_DECK_PRODUCT {
        return PadKind::SteamDeck;
    }
    match ty {
        SDL_GAMEPAD_TYPE_XBOX360 => PadKind::Xbox360,
        SDL_GAMEPAD_TYPE_XBOXONE => PadKind::XboxSeries,
        SDL_GAMEPAD_TYPE_PS3 => PadKind::PlayStation3,
        SDL_GAMEPAD_TYPE_PS4 => PadKind::PlayStation4,
        SDL_GAMEPAD_TYPE_PS5 => PadKind::PlayStation5,
        SDL_GAMEPAD_TYPE_NINTENDO_SWITCH_PRO
        | SDL_GAMEPAD_TYPE_NINTENDO_SWITCH_JOYCON_LEFT
        | SDL_GAMEPAD_TYPE_NINTENDO_SWITCH_JOYCON_RIGHT
        | SDL_GAMEPAD_TYPE_NINTENDO_SWITCH_JOYCON_PAIR => PadKind::Switch,
        SDL_GAMEPAD_TYPE_GAMECUBE => PadKind::GameCube,
        SDL_GAMEPAD_TYPE_STEAM => PadKind::SteamController,
        _ if vendor == VALVE_VENDOR && STEAM_CONTROLLER_PRODUCTS.contains(&product) => PadKind::SteamController,
        _ if !is_gamepad => PadKind::Generic,
        // Standard/unknown gamepads use the Xbox layout (SDL's own convention).
        _ => PadKind::XboxSeries,
    }
}

/// Axis travel (of 32767) that counts as the player using a pad, so a resting stick's drift
/// doesn't take the prompts away from the keyboard.
pub const ACTIVE_AXIS_THRESHOLD: i16 = 16000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_pads() {
        assert_eq!(classify(true, SDL_GAMEPAD_TYPE_PS5, 0x054C, 0x0CE6), PadKind::PlayStation5);
        assert_eq!(classify(true, SDL_GAMEPAD_TYPE_PS4, 0x054C, 0x09CC), PadKind::PlayStation4);
        assert_eq!(classify(true, SDL_GAMEPAD_TYPE_XBOXONE, 0x045E, 0x0B12), PadKind::XboxSeries);
        assert_eq!(classify(true, SDL_GAMEPAD_TYPE_XBOX360, 0x045E, 0x028E), PadKind::Xbox360);
        assert_eq!(classify(true, SDL_GAMEPAD_TYPE_NINTENDO_SWITCH_PRO, 0x057E, 0x2009), PadKind::Switch);
        assert_eq!(classify(true, SDL_GAMEPAD_TYPE_NINTENDO_SWITCH_JOYCON_PAIR, 0x057E, 0x2008), PadKind::Switch);
        assert_eq!(classify(true, SDL_GAMEPAD_TYPE_GAMECUBE, 0x057E, 0x0337), PadKind::GameCube);
        assert_eq!(classify(true, SDL_GAMEPAD_TYPE_STANDARD, 0x2DC8, 0x6101), PadKind::XboxSeries);
        assert_eq!(classify(false, SDL_GAMEPAD_TYPE_UNKNOWN, 0x044F, 0xB10A), PadKind::Generic);
    }

    #[test]
    fn classifies_valve_hardware() {
        // The Deck is identified by its ids whatever type SDL reports.
        assert_eq!(classify(true, SDL_GAMEPAD_TYPE_STANDARD, VALVE_VENDOR, STEAM_DECK_PRODUCT), PadKind::SteamDeck);
        assert_eq!(classify(true, SDL_GAMEPAD_TYPE_STEAM, VALVE_VENDOR, STEAM_DECK_PRODUCT), PadKind::SteamDeck);
        assert_eq!(classify(true, SDL_GAMEPAD_TYPE_STEAM, VALVE_VENDOR, 0x1142), PadKind::SteamController);
        assert_eq!(classify(true, SDL_GAMEPAD_TYPE_STANDARD, VALVE_VENDOR, 0x1102), PadKind::SteamController);
        // Steam Input's virtual pad: the type Steam passes through wins over the Valve vendor id.
        assert_eq!(classify(true, SDL_GAMEPAD_TYPE_PS5, VALVE_VENDOR, 0x11FF), PadKind::PlayStation5);
        assert_eq!(classify(true, SDL_GAMEPAD_TYPE_XBOX360, VALVE_VENDOR, 0x11FF), PadKind::Xbox360);
    }
}
