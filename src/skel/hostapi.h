#pragma once

// Platform API used by game code for mouse, window and joystick queries.
//
// Two implementations:
//   - Rust host (rust/revc, SDL3), selected with LIBRW_HOST. The skeleton is skel/host/host.cpp.
//   - skel/glfw/glfw.cpp, thin GLFW wrappers, kept for side-by-side comparison until it is deleted.
//
// Joystick ids are slots 0..HOST_JOYSTICK_LAST. Gamepad state uses GLFW's button/axis layout
// (ControllerConfig.cpp maps those indices to PS2 buttons), with triggers in [-1, 1], -1 = released.

#ifdef __cplusplus
extern "C" {
#endif

enum {
	HOST_JOYSTICK_LAST = 15,

	HOST_MOUSE_BUTTON_LEFT = 0,
	HOST_MOUSE_BUTTON_RIGHT = 1,
	HOST_MOUSE_BUTTON_MIDDLE = 2,
	HOST_MOUSE_BUTTON_4 = 3,
	HOST_MOUSE_BUTTON_5 = 4,

	HOST_CURSOR_NORMAL = 0,
	HOST_CURSOR_HIDDEN = 1,
	HOST_CURSOR_DISABLED = 2, // hidden, captured, unbounded relative motion

	HOST_PAD_BUTTON_A = 0,
	HOST_PAD_BUTTON_B = 1,
	HOST_PAD_BUTTON_X = 2,
	HOST_PAD_BUTTON_Y = 3,
	HOST_PAD_BUTTON_LEFT_BUMPER = 4,
	HOST_PAD_BUTTON_RIGHT_BUMPER = 5,
	HOST_PAD_BUTTON_BACK = 6,
	HOST_PAD_BUTTON_START = 7,
	HOST_PAD_BUTTON_GUIDE = 8,
	HOST_PAD_BUTTON_LEFT_THUMB = 9,
	HOST_PAD_BUTTON_RIGHT_THUMB = 10,
	HOST_PAD_BUTTON_DPAD_UP = 11,
	HOST_PAD_BUTTON_DPAD_RIGHT = 12,
	HOST_PAD_BUTTON_DPAD_DOWN = 13,
	HOST_PAD_BUTTON_DPAD_LEFT = 14,
	HOST_PAD_BUTTON_COUNT = 15,

	HOST_PAD_AXIS_LEFT_X = 0,
	HOST_PAD_AXIS_LEFT_Y = 1,
	HOST_PAD_AXIS_RIGHT_X = 2,
	HOST_PAD_AXIS_RIGHT_Y = 3,
	HOST_PAD_AXIS_LEFT_TRIGGER = 4,
	HOST_PAD_AXIS_RIGHT_TRIGGER = 5,
	HOST_PAD_AXIS_COUNT = 6,
};

typedef struct HostGamepadState {
	unsigned char buttons[HOST_PAD_BUTTON_COUNT];
	float axes[HOST_PAD_AXIS_COUNT];
} HostGamepadState;

// Mouse. Positions are in window coordinates.
void hostGetCursorPos(double *x, double *y);
void hostSetCursorPos(double x, double y);
int hostGetMouseButton(int button);

// Joysticks
int hostJoystickPresent(int jid);
const char *hostGetJoystickName(int jid); // NULL if not present
const unsigned char *hostGetJoystickButtons(int jid, int *count);

#ifdef LIBRW_HOST
// Only used by skel/host/host.cpp
void hostPollEvents(void);
int hostWindowShouldClose(void);
void hostSetCursorMode(int mode);
void hostGetWindowSize(int *w, int *h);
void hostSetWindowSize(int w, int h);
void hostGetDesktopMode(int *w, int *h, int *refreshRate);
// Switch the existing window between fullscreen (exclusive mode width x height, or
// borderless desktop if no such mode) and windowed (width x height), keeping the GL context.
int hostSetFullscreen(int fullscreen, int width, int height);
int hostJoystickIsGamepad(int jid);
const float *hostGetJoystickAxes(int jid, int *count);
int hostGetGamepadState(int jid, HostGamepadState *state);
int hostUpdateGamepadMappings(const char *mappings);

// Implemented in host.cpp, called by the host from inside hostPollEvents().
// key is an RsKeyCodes value (skeleton.h).
void revc_on_key(int key, int down);
void revc_on_resize(int width, int height);
void revc_on_scroll(double dx, double dy);
void revc_on_cursor(double x, double y);
void revc_on_cursor_enter(int entered);
void revc_on_focus(int focused);
void revc_on_iconify(int iconified);
void revc_on_joystick(int jid, int connected);
void revc_on_toggle_fullscreen(void); // Alt+Enter
#endif

#ifdef __cplusplus
}
#endif
