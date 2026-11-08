//! Joystick/gamepad slots for the host API, with GLFW semantics (slot ids 0..15,
//! raw buttons + hat buttons, gamepad state in GLFW layout). First draft by the local model.

use sdl3_sys::everything::*;
use std::ffi::{CStr, CString, c_char};

pub const MAX_SLOTS: usize = 16; // GLFW_JOYSTICK_LAST + 1

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct HostGamepadState {
    pub buttons: [u8; 15],
    pub axes: [f32; 6],
}

struct Slot {
    id: SDL_JoystickID,
    joy: *mut SDL_Joystick,
    pad: *mut SDL_Gamepad,
    name: CString,
    buttons: Vec<u8>,
    axes: Vec<f32>,
}

pub struct Joysticks {
    slots: [Option<Slot>; MAX_SLOTS],
}

impl Default for Joysticks {
    fn default() -> Self {
        Joysticks {
            slots: std::array::from_fn(|_| None),
        }
    }
}

impl Joysticks {
    pub fn add(&mut self, id: SDL_JoystickID) -> Option<usize> {
        // Check if already present
        for slot in self.slots.iter() {
            if let Some(s) = slot {
                if s.id == id {
                    return None;
                }
            }
        }

        // Find first free slot
        let mut slot_idx = None;
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if slot.is_none() {
                slot_idx = Some(i);
                break;
            }
        }

        let slot_idx = slot_idx?;

        // Open joystick
        let joy = unsafe { SDL_OpenJoystick(id) };
        if joy.is_null() {
            return None;
        }

        let is_gamepad = unsafe { SDL_IsGamepad(id) };
        let mut pad: *mut SDL_Gamepad = std::ptr::null_mut();
        if is_gamepad {
            pad = unsafe { SDL_OpenGamepad(id) };
            // If opening as a gamepad fails, it is still usable as a raw joystick.
        }

        // Get name
        let name_ptr = unsafe { SDL_GetJoystickName(joy) };
        let name = if name_ptr.is_null() {
            CString::new("Joystick").unwrap()
        } else {
            unsafe { CStr::from_ptr(name_ptr).to_owned() }
        };

        // Initial button/axis collection
        let mut buttons = Vec::new();
        let mut axes = Vec::new();
        refresh_joystick_state(joy, &mut buttons, &mut axes);

        let slot = Slot {
            id,
            joy,
            pad,
            name,
            buttons,
            axes,
        };

        self.slots[slot_idx] = Some(slot);
        Some(slot_idx)
    }

    pub fn remove(&mut self, id: SDL_JoystickID) -> Option<usize> {
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if slot.as_ref().is_some_and(|s| s.id == id) {
                {
                    let s = slot.take().unwrap();
                    if !s.pad.is_null() {
                        unsafe { SDL_CloseGamepad(s.pad) };
                    }
                    if !s.joy.is_null() {
                        unsafe { SDL_CloseJoystick(s.joy) };
                    }
                    return Some(i);
                }
            }
        }
        None
    }

    /// Re-check raw joysticks after gamepad mappings were added, opening any that
    /// SDL now recognises as gamepads (GLFW applies new mappings to open devices too).
    pub fn refresh_gamepads(&mut self) {
        for s in self.slots.iter_mut().flatten() {
            if s.pad.is_null() && unsafe { SDL_IsGamepad(s.id) } {
                s.pad = unsafe { SDL_OpenGamepad(s.id) };
            }
        }
    }

    pub fn present(&self, slot: i32) -> bool {
        if slot < 0 || slot as usize >= MAX_SLOTS {
            return false;
        }
        self.slots[slot as usize].is_some()
    }

    pub fn is_gamepad(&self, slot: i32) -> bool {
        if slot < 0 || slot as usize >= MAX_SLOTS {
            return false;
        }
        match &self.slots[slot as usize] {
            Some(s) => !s.pad.is_null(),
            None => false,
        }
    }

    pub fn name(&self, slot: i32) -> *const c_char {
        if slot < 0 || slot as usize >= MAX_SLOTS {
            return std::ptr::null();
        }
        match &self.slots[slot as usize] {
            Some(s) => s.name.as_ptr(),
            None => std::ptr::null(),
        }
    }

    pub fn buttons(&mut self, slot: i32) -> (*const u8, i32) {
        if slot < 0 || slot as usize >= MAX_SLOTS {
            return (std::ptr::null(), 0);
        }
        let Some(s) = self.slots[slot as usize].as_mut() else {
            return (std::ptr::null(), 0);
        };
        refresh_joystick_state(s.joy, &mut s.buttons, &mut s.axes);
        (s.buttons.as_ptr(), s.buttons.len() as i32)
    }

    pub fn axes(&mut self, slot: i32) -> (*const f32, i32) {
        if slot < 0 || slot as usize >= MAX_SLOTS {
            return (std::ptr::null(), 0);
        }
        let Some(s) = self.slots[slot as usize].as_mut() else {
            return (std::ptr::null(), 0);
        };
        refresh_joystick_state(s.joy, &mut s.buttons, &mut s.axes);
        (s.axes.as_ptr(), s.axes.len() as i32)
    }

    pub fn gamepad_state(&self, slot: i32) -> Option<HostGamepadState> {
        if slot < 0 || slot as usize >= MAX_SLOTS {
            return None;
        }
        let s = self.slots[slot as usize].as_ref()?;
        if s.pad.is_null() {
            return None;
        }

        let mut state = HostGamepadState::default();

        // Buttons mapping:
        // 0 SOUTH, 1 EAST, 2 WEST, 3 NORTH, 4 LEFT_SHOULDER, 5 RIGHT_SHOULDER,
        // 6 BACK, 7 START, 8 GUIDE, 9 LEFT_STICK, 10 RIGHT_STICK,
        // 11 DPAD_UP, 12 DPAD_RIGHT, 13 DPAD_DOWN, 14 DPAD_LEFT

        let map_btn = |g: *mut SDL_Gamepad, sdl_btn: SDL_GamepadButton| -> u8 {
            unsafe { SDL_GetGamepadButton(g, sdl_btn) as u8 }
        };

        state.buttons[0] = map_btn(s.pad, SDL_GAMEPAD_BUTTON_SOUTH);
        state.buttons[1] = map_btn(s.pad, SDL_GAMEPAD_BUTTON_EAST);
        state.buttons[2] = map_btn(s.pad, SDL_GAMEPAD_BUTTON_WEST);
        state.buttons[3] = map_btn(s.pad, SDL_GAMEPAD_BUTTON_NORTH);
        state.buttons[4] = map_btn(s.pad, SDL_GAMEPAD_BUTTON_LEFT_SHOULDER);
        state.buttons[5] = map_btn(s.pad, SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER);
        state.buttons[6] = map_btn(s.pad, SDL_GAMEPAD_BUTTON_BACK);
        state.buttons[7] = map_btn(s.pad, SDL_GAMEPAD_BUTTON_START);
        state.buttons[8] = map_btn(s.pad, SDL_GAMEPAD_BUTTON_GUIDE);
        state.buttons[9] = map_btn(s.pad, SDL_GAMEPAD_BUTTON_LEFT_STICK);
        state.buttons[10] = map_btn(s.pad, SDL_GAMEPAD_BUTTON_RIGHT_STICK);
        state.buttons[11] = map_btn(s.pad, SDL_GAMEPAD_BUTTON_DPAD_UP);
        state.buttons[12] = map_btn(s.pad, SDL_GAMEPAD_BUTTON_DPAD_RIGHT);
        state.buttons[13] = map_btn(s.pad, SDL_GAMEPAD_BUTTON_DPAD_DOWN);
        state.buttons[14] = map_btn(s.pad, SDL_GAMEPAD_BUTTON_DPAD_LEFT);

        // Axes mapping:
        // 0 LEFTX, 1 LEFTY, 2 RIGHTX, 3 RIGHTY, 4 LEFT_TRIGGER, 5 RIGHT_TRIGGER

        let map_axis = |g: *mut SDL_Gamepad, sdl_axis: SDL_GamepadAxis| -> f32 {
            let v = unsafe { SDL_GetGamepadAxis(g, sdl_axis) };
            (v as f32 / 32767.0).clamp(-1.0, 1.0)
        };

        let map_trigger = |g: *mut SDL_Gamepad, sdl_axis: SDL_GamepadAxis| -> f32 {
            let v = unsafe { SDL_GetGamepadAxis(g, sdl_axis) };
            // SDL triggers are 0..32767. GLFW expects -1..1 where -1 is released.
            let val = (v as f32 / 32767.0) * 2.0 - 1.0;
            val.clamp(-1.0, 1.0)
        };

        state.axes[0] = map_axis(s.pad, SDL_GAMEPAD_AXIS_LEFTX);
        state.axes[1] = map_axis(s.pad, SDL_GAMEPAD_AXIS_LEFTY);
        state.axes[2] = map_axis(s.pad, SDL_GAMEPAD_AXIS_RIGHTX);
        state.axes[3] = map_axis(s.pad, SDL_GAMEPAD_AXIS_RIGHTY);
        state.axes[4] = map_trigger(s.pad, SDL_GAMEPAD_AXIS_LEFT_TRIGGER);
        state.axes[5] = map_trigger(s.pad, SDL_GAMEPAD_AXIS_RIGHT_TRIGGER);

        Some(state)
    }
}

impl Drop for Joysticks {
    fn drop(&mut self) {
        for slot in self.slots.iter_mut() {
            if let Some(s) = slot {
                if !s.pad.is_null() {
                    unsafe { SDL_CloseGamepad(s.pad) };
                }
                if !s.joy.is_null() {
                    unsafe { SDL_CloseJoystick(s.joy) };
                }
            }
        }
    }
}

fn refresh_joystick_state(joy: *mut SDL_Joystick, buttons: &mut Vec<u8>, axes: &mut Vec<f32>) {
    // Raw state, GLFW style: buttons, then 4 per hat (up, right, down, left).
    // Clear previous data
    buttons.clear();
    axes.clear();

    // Collect joystick buttons
    if !joy.is_null() {
        let num_btns = unsafe { SDL_GetNumJoystickButtons(joy) };
        for i in 0..num_btns {
            let pressed = unsafe { SDL_GetJoystickButton(joy, i) };
            buttons.push(if pressed { 1 } else { 0 });
        }

        // Collect joystick axes
        let num_axes = unsafe { SDL_GetNumJoystickAxes(joy) };
        for i in 0..num_axes {
            let val = unsafe { SDL_GetJoystickAxis(joy, i) };
            let norm = (val as f32 / 32767.0).clamp(-1.0, 1.0);
            axes.push(norm);
        }

        // Collect hats
        let num_hats = unsafe { SDL_GetNumJoystickHats(joy) };
        for i in 0..num_hats {
            let hat = unsafe { SDL_GetJoystickHat(joy, i) };
            // Up
            buttons.push(if hat & SDL_HAT_UP != 0 { 1 } else { 0 });
            // Right
            buttons.push(if hat & SDL_HAT_RIGHT != 0 { 1 } else { 0 });
            // Down
            buttons.push(if hat & SDL_HAT_DOWN != 0 { 1 } else { 0 });
            // Left
            buttons.push(if hat & SDL_HAT_LEFT != 0 { 1 } else { 0 });
        }
    }
}
