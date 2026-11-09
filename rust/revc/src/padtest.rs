//! Controller test with SDL virtual pads (`REVC_TEST=gamepad`, plan task 9). No hardware needed:
//! SDL3 virtual joysticks behave like real ones (hot-plug events, gamepad type from the USB ids,
//! rumble requests), so this checks the whole path the game uses:
//!   1. plug in a virtual DualSense: the host reports a PS5 pad, the game takes it as player 1;
//!   2. press a button on it: the active input becomes the PS5 pad and the prompt glyphs are
//!      reloaded from the PlayStation set; a help box with every button token is captured
//!      (gamepad_ps5.png, compared with tests/reference/gamepad/);
//!   3. plug in a Steam Deck and press a button: player 2 pad, glyphs switch to the Steam Deck set
//!      (gamepad_deck.png);
//!   4. Controls menu "BUTTON PROMPTS" = Nintendo while the Deck is in use: the Switch set (B on the
//!      bottom button) is shown instead (gamepad_override_nintendo.png); back to AUTO: Steam Deck;
//!   5. use the mouse: the active input goes back to keyboard + mouse;
//!   5. CPad::StartShake on player 1: the DualSense gets a rumble request, then a stop;
//!   6. unplug both: the game releases them.

use crate::host::{self, PadKind};
use sdl3_sys::everything::*;
use std::ffi::c_void;
use std::sync::Mutex;

unsafe extern "C" {
    fn revc_test_joy_ids(joy1: *mut i32, joy2: *mut i32);
    fn revc_test_shake(duration_ms: i32, freq: i32);
    fn hostJoystickKind(jid: i32) -> i32;
    fn hostJoystickPresent(jid: i32) -> i32;
    fn revc_test_show_buttons();
    fn revc_test_glyph_sets_loaded() -> i32;
    fn revc_test_game_ms() -> u32;
    fn revc_test_set_prompt_style(style: i32);
}

/// Rumble requests received by the virtual DualSense: (low, high).
static RUMBLE: Mutex<Vec<(u16, u16)>> = Mutex::new(Vec::new());

unsafe extern "C" fn on_rumble(_userdata: *mut c_void, low: u16, high: u16) -> bool {
    RUMBLE.lock().unwrap().push((low, high));
    true
}

fn attach(name: &'static std::ffi::CStr, vendor: u16, product: u16, rumble: bool) -> Result<SDL_JoystickID, String> {
    let mut desc = SDL_VirtualJoystickDesc::new();
    desc.r#type = SDL_JOYSTICK_TYPE_GAMEPAD.0 as u16;
    desc.vendor_id = vendor;
    desc.product_id = product;
    desc.naxes = SDL_GAMEPAD_AXIS_COUNT.0 as u16;
    desc.nbuttons = SDL_GAMEPAD_BUTTON_COUNT.0 as u16;
    desc.name = name.as_ptr();
    if rumble {
        desc.Rumble = Some(on_rumble);
    }
    let id = unsafe { SDL_AttachVirtualJoystick(&desc) };
    if id.0 == 0 {
        return Err(format!("SDL_AttachVirtualJoystick failed: {}", sdl_error()));
    }
    Ok(id)
}

fn sdl_error() -> String {
    unsafe { std::ffi::CStr::from_ptr(SDL_GetError()) }.to_string_lossy().into_owned()
}

fn joy_ids() -> (i32, i32) {
    let (mut a, mut b) = (-1, -1);
    unsafe { revc_test_joy_ids(&mut a, &mut b) };
    (a, b)
}

/// Host slot holding a pad of this kind, if any.
fn slot_of(kind: PadKind) -> Option<i32> {
    (0..16).find(|&s| unsafe { hostJoystickPresent(s) } != 0 && unsafe { hostJoystickKind(s) } == kind as i32)
}

fn press(id: SDL_JoystickID, down: bool) -> Result<(), String> {
    // Virtual state is set through an open handle; the host already has the device open, so this
    // returns the same joystick (reference counted) and the close below keeps it open.
    let joy = unsafe { SDL_OpenJoystick(id) };
    if joy.is_null() {
        return Err(format!("SDL_OpenJoystick: {}", sdl_error()));
    }
    let ok = unsafe { SDL_SetJoystickVirtualButton(joy, SDL_GAMEPAD_BUTTON_SOUTH.0, down) };
    unsafe { SDL_CloseJoystick(joy) };
    if ok { Ok(()) } else { Err(format!("SDL_SetJoystickVirtualButton: {}", sdl_error())) }
}

fn mouse_wheel() {
    let mut ev = SDL_Event::default();
    ev.r#type = SDL_EVENT_MOUSE_WHEEL.0;
    unsafe { SDL_PushEvent(&mut ev) };
}

#[derive(Default)]
pub struct PadTest {
    step: u32,
    /// Frames spent in the current step.
    frames: u32,
    /// Game time the current step started (for waits).
    t0: u32,
    ps5: Option<SDL_JoystickID>,
    deck: Option<SDL_JoystickID>,
    pub log: Vec<String>,
    /// Screenshot wanted now (name); the driver captures it and clears this.
    pub capture: Option<&'static str>,
}

/// Frames a step may take before it counts as failed (events arrive within a frame or two).
const STEP_LIMIT: u32 = 120;
/// Help box fade-in before a glyph screenshot.
const HELP_SETTLE_MS: u32 = 1500;

impl PadTest {
    fn next(&mut self) {
        self.step += 1;
        self.frames = 0;
        self.t0 = unsafe { revc_test_game_ms() };
    }

    /// Whether the prompt glyphs have been reloaded for `kind` (at least `sets` loads so far); the
    /// font code does that at the start of the frame after the pad was used.
    fn glyphs_loaded(&mut self, kind: PadKind, sets: i32) -> bool {
        let name = host::glyph_set_name();
        let loaded = unsafe { revc_test_glyph_sets_loaded() };
        if name == format!("{kind:?}") && loaded >= sets {
            self.log.push(format!("prompt glyphs: {name} set loaded"));
            unsafe { revc_test_show_buttons() };
            true
        } else {
            false
        }
    }

    /// Advances one frame. Ok(true) when finished, Err on failure.
    pub fn tick(&mut self) -> Result<bool, String> {
        if self.capture.is_some() {
            return Ok(false); // waiting for the driver's screenshot
        }
        self.frames += 1;
        let waited_too_long = self.frames > STEP_LIMIT;
        let settled = unsafe { revc_test_game_ms() }.wrapping_sub(self.t0) >= HELP_SETTLE_MS;
        match self.step {
            0 => {
                RUMBLE.lock().unwrap().clear();
                self.ps5 = Some(attach(c"revc test DualSense", 0x054C, 0x0CE6, true)?);
                self.next();
            }
            1 => {
                let slot = slot_of(PadKind::PlayStation5);
                let (j1, _) = joy_ids();
                if let Some(s) = slot.filter(|&s| s == j1) {
                    self.log.push(format!("DualSense in slot {s}, kind PS5, player 1"));
                    press(self.ps5.unwrap(), true)?;
                    self.next();
                } else if waited_too_long {
                    return Err(format!("virtual DualSense not taken as PS5 player-1 pad (slot {slot:?}, joy1 {j1})"));
                }
            }
            2 => {
                if host::active_kind() == PadKind::PlayStation5 {
                    press(self.ps5.unwrap(), false)?;
                    self.log.push("button on DualSense -> active input PS5".into());
                    self.next();
                } else if waited_too_long {
                    return Err(format!("active input after DualSense button is {:?}", host::active_kind()));
                }
            }
            3 => {
                if self.glyphs_loaded(PadKind::PlayStation5, 1) {
                    self.next();
                } else if waited_too_long {
                    return Err(format!("prompt glyphs not reloaded for PS5 (set {})", host::glyph_set_name()));
                }
            }
            4 if settled => {
                self.capture = Some("gamepad_ps5");
                self.next();
            }
            5 => {
                self.deck = Some(attach(c"revc test Steam Deck", 0x28DE, 0x1205, false)?);
                self.next();
            }
            6 => {
                let slot = slot_of(PadKind::SteamDeck);
                let (_, j2) = joy_ids();
                if let Some(s) = slot.filter(|&s| s == j2) {
                    self.log.push(format!("Steam Deck in slot {s}, player 2"));
                    press(self.deck.unwrap(), true)?;
                    self.next();
                } else if waited_too_long {
                    return Err(format!("virtual Steam Deck not taken as player-2 pad (slot {slot:?}, joy2 {j2})"));
                }
            }
            7 => {
                if host::active_kind() == PadKind::SteamDeck {
                    press(self.deck.unwrap(), false)?;
                    self.log.push("button on Steam Deck -> active input Steam Deck".into());
                    self.next();
                } else if waited_too_long {
                    return Err(format!("active input after Steam Deck button is {:?}", host::active_kind()));
                }
            }
            8 => {
                if self.glyphs_loaded(PadKind::SteamDeck, 2) {
                    self.next();
                } else if waited_too_long {
                    return Err(format!("prompt glyphs not reloaded for Steam Deck (set {})", host::glyph_set_name()));
                }
            }
            9 if settled => {
                self.capture = Some("gamepad_deck");
                self.next();
            }
            10 => {
                unsafe { revc_test_set_prompt_style(3) }; // Nintendo
                self.next();
            }
            11 => {
                if self.glyphs_loaded(PadKind::Switch, 3) {
                    self.next();
                } else if waited_too_long {
                    return Err(format!("menu override Nintendo not applied (set {})", host::glyph_set_name()));
                }
            }
            12 if settled => {
                self.capture = Some("gamepad_override_nintendo");
                self.next();
            }
            13 => {
                unsafe { revc_test_set_prompt_style(0) }; // AUTO
                self.next();
            }
            14 => {
                if host::glyph_set_name() == "SteamDeck" && unsafe { revc_test_glyph_sets_loaded() } >= 4 {
                    self.log.push("menu override back to AUTO -> Steam Deck set".into());
                    self.next();
                } else if waited_too_long {
                    return Err(format!("AUTO after override: set {}", host::glyph_set_name()));
                }
            }
            15 => {
                mouse_wheel();
                self.next();
            }
            16 => {
                if host::active_kind() == PadKind::KeyboardMouse {
                    self.log.push("mouse -> active input keyboard+mouse".into());
                    unsafe { revc_test_shake(500, 200) };
                    self.next();
                } else if waited_too_long {
                    return Err(format!("active input after mouse wheel is {:?}", host::active_kind()));
                }
            }
            17 => {
                // 500 ms of shake is about 15 steps at 30 Hz; allow for slow frames.
                let log = RUMBLE.lock().unwrap().clone();
                let started = log.iter().position(|&(l, h)| l > 0 && h > 0);
                let stopped = started.is_some_and(|i| log[i..].iter().any(|&(l, h)| l == 0 && h == 0));
                if stopped {
                    let strength = log[started.unwrap()];
                    self.log.push(format!("shake -> rumble {strength:?}, then stop ({} requests)", log.len()));
                    // 200/255 of full strength, as the XInput version computes it.
                    if strength.0 != (200.0f32 / 255.0 * 65535.0) as u16 {
                        return Err(format!("rumble strength {strength:?}, expected 200/255 of 65535"));
                    }
                    unsafe {
                        SDL_DetachVirtualJoystick(self.ps5.unwrap());
                        SDL_DetachVirtualJoystick(self.deck.unwrap());
                    }
                    self.next();
                } else if self.frames > STEP_LIMIT * 3 {
                    return Err(format!("rumble requests {log:?}: expected a start and a stop"));
                }
            }
            18 => {
                let (j1, j2) = joy_ids();
                if j1 == -1 && j2 == -1 && slot_of(PadKind::PlayStation5).is_none() && slot_of(PadKind::SteamDeck).is_none() {
                    self.log.push("unplugged -> game released both pads".into());
                    return Ok(true);
                } else if waited_too_long {
                    return Err(format!("after unplugging: joy1 {j1}, joy2 {j2}"));
                }
            }
            4 | 9 | 12 => {} // waiting for the help box
            _ => return Ok(true),
        }
        Ok(false)
    }
}
