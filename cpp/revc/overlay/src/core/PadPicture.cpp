// revc: the controller layout picture (MENUPAGE_PAD_PICTURE), as on the Xbox version's controls
// screen: the pad outline with a callout to each button saying what it does, on foot or in a car,
// for the bindings in use (Xbox Standard / Xbox Classic / Original PC / Custom). The pad outline is
// the Xbox's own (R3_menu_336), imported from the player's disc (Options > CONSOLE DISCS,
// goal "Asset import from original discs"); without it the callouts are drawn alone and the
// player is asked for the disc once.
#include "common.h"
#include "Frontend.h"
#include "Font.h"
#include "Sprite2d.h"
#include "Text.h"
#include "ControllerConfig.h"

// The menu's 640x448 layout coordinates (as Frontend.cpp defines them).
#ifdef ASPECT_RATIO_SCALE
#define MENU_X_LEFT_ALIGNED(x) SCALE_AND_CENTER_X(x)
#define MENU_X(x) SCREEN_SCALE_X(x)
#define MENU_Y(y) SCREEN_SCALE_Y(y)
#else
#define MENU_X_LEFT_ALIGNED(x) StretchX(x)
#define MENU_X(x) StretchX(x)
#define MENU_Y(y) StretchY(y)
#endif
static const CRGBA MENUOPTION_COLOR(255, 150, 225, 255);

extern "C" int revc_import_image(const char *rel, int *w, int *h, unsigned char **rgba);
extern "C" void revc_texmod_free(unsigned char *rgba, int width, int height);
extern "C" int revc_import_status(int kind);
extern "C" int revc_import_busy(void);
extern "C" void revc_import_prompt(void);
extern "C" int PadLayoutCurrent(void);

static int8 padContext;	// 0 on foot, 1 in car, 2 flying (as the Xbox's Walking / Driving / Flying)
// Heap, never destroyed: a static CSprite2d would free its texture after RenderWare shut down.
static CSprite2d *padSprite;
static bool padPrompted;

wchar*
PadPictureContextDraw(bool *disabled, bool userHovering)
{
	return TheText.Get(padContext == 0 ? "RVC_PPF" : padContext == 1 ? "RVC_PPC" : "RVC_PPH");
}

// Tests (menu comparison shots): pick the context directly.
extern "C" void
PadPictureSetContext(int context)
{
	padContext = context % 3;
}

void
PadPictureContextPress(int8 action)
{
	if (action == FEOPTION_ACTION_LEFT)
		padContext = (padContext + 2) % 3;
	else if (action == FEOPTION_ACTION_RIGHT || action == FEOPTION_ACTION_SELECT)
		padContext = (padContext + 1) % 3;
}

// The imported pad outline as a sprite (once the import is there).
static void
LoadPadSprite(void)
{
	if (padSprite == nil)
		padSprite = new CSprite2d;
	if (padSprite->m_pTexture || !revc_import_status(0))
		return;
	int w, h;
	unsigned char *px;
	if (!revc_import_image("xbox/ui/r3_menu_336.dds", &w, &h, &px))
		return;
	rw::Image *img = rw::Image::create(w, h, 32);
	img->pixels = px;
	img->stride = w * 4;
	rw::Raster *ras = rw::Raster::create(w, h, 32, rw::Raster::C8888 | rw::Raster::TEXTURE);
	if (ras && ras->setFromImage(img)) {
		rw::Texture *tex = rw::Texture::create(ras);
		strncpy(tex->name, "xboxpad", sizeof(tex->name) - 1);
		tex->setFilter(rw::Texture::LINEAR);
		padSprite->m_pTexture = tex;
	} else if (ras)
		ras->destroy();
	img->pixels = nil;
	img->destroy();
	revc_texmod_free(px, w, h);
}

// Where each control is on the 512x256 outline, and its glyph token. Joystick numbers are the
// game's (1 B, 2 A, 3 X, 4 Y, 5 LT, 6 RT, 7 LB/White, 8 RB/Black, 9 Back, 10 L3, 11 R3, 12 Start,
// 13-16 D-pad up/right/down/left); 20/21 are the left/right sticks themselves.
struct PadSpot { int joy; float x, y; const char *glyph; };
static const PadSpot padSpots[] = {
	{ 4, 388, 60, "~T~" }, { 1, 410, 82, "~O~" }, { 3, 366, 82, "~Q~" }, { 2, 388, 104, "~X~" },
	{ 5, 112, 18, "~M~" }, { 6, 400, 18, "~V~" },
	{ 7, 432, 121, "~K~" }, { 8, 402, 141, "~J~" },
	{ 9, 78, 112, "~S~" }, { 12, 101, 131, "~P~" },
	{ 10, 108, 82, "~A~" }, { 11, 318, 138, "~C~" },
	{ 13, 178, 124, "~E~" }, { 14, 192, 138, "~F~" }, { 15, 178, 152, "~G~" }, { 16, 164, 138, "~I~" },
	{ 20, 96, 70, "" }, { 21, 330, 126, "" },
};

// What is shown for each context: an action and its name (GXT key).
// contexts: bit 1 on foot, 2 in car, 4 flying. Flying is the Xbox's own page: throttle up and
// down, fire, sub-missions, radio, camera, enter/exit (no horn, handbrake or look left/right).
struct PadAction { e_ControllerAction action; const char *text; int8 contexts; };
enum { FOOT = 1, CAR = 2, FLY = 4 };
static const PadAction padActions[] = {
	{ PED_FIREWEAPON, "FEC_FIR", FOOT }, { PED_LOCK_TARGET, "FEC_TAR", FOOT }, { PED_SPRINT, "FEC_SPN", FOOT },
	{ PED_JUMPING, "FEC_JMP", FOOT }, { VEHICLE_ENTER_EXIT, "FEC_EEX", FOOT | CAR | FLY }, { PED_CYCLE_WEAPON_RIGHT, "FEC_NWE", FOOT },
	{ PED_CYCLE_WEAPON_LEFT, "FEC_PWE", FOOT }, { PED_ANSWER_PHONE, "FEC_ANS", FOOT }, { PED_DUCK, "FEC_CRO", FOOT },
	{ PED_LOOKBEHIND, "FEC_LBA", FOOT }, { CAMERA_CHANGE_VIEW_ALL_SITUATIONS, "FEC_CMR", FOOT | CAR | FLY },
	{ PED_CENTER_CAMERA_BEHIND_PLAYER, "FEC_CEN", FOOT }, { PED_CYCLE_TARGET_RIGHT, "FEC_NTR", FOOT },
	{ PED_CYCLE_TARGET_LEFT, "FEC_PTT", FOOT }, { GO_FORWARD, "FEC_FOR", FOOT }, { GO_BACK, "FEC_BAC", FOOT },
	{ GO_LEFT, "FEC_LEF", FOOT }, { GO_RIGHT, "FEC_RIG", FOOT },
	{ VEHICLE_ACCELERATE, "RVC_ACC", CAR | FLY }, { VEHICLE_BRAKE, "RVC_BRK", CAR }, { VEHICLE_BRAKE, "RVC_DEC", FLY },
	{ VEHICLE_HANDBRAKE, "FEC_HND", CAR },
	{ VEHICLE_FIREWEAPON, "FEC_FIR", CAR | FLY }, { VEHICLE_HORN, "FEC_HRN", CAR }, { TOGGLE_SUBMISSIONS, "FEC_SUB", CAR | FLY },
	{ VEHICLE_CHANGE_RADIO_STATION, "FEC_RAD", CAR | FLY }, { VEHICLE_LOOKLEFT, "FEC_LOL", CAR }, { VEHICLE_LOOKRIGHT, "FEC_LOR", CAR },
};

struct Callout { const PadSpot *spot; wchar text[96]; };

static void
AppendText(wchar *dst, const char *key, int max)
{
	wchar *t = TheText.Get(key);
	int n = UnicodeStrlen(dst);
	if (n > 0 && n + 3 < max) {
		AsciiToUnicode(" / ", dst + n);
		n += 3;
	}
	for (int i = 0; t[i] && n < max - 1; i++)
		dst[n++] = t[i];
	dst[n] = 0;
}

// A thin line from (x1,y1) to (x2,y2), screen pixels.
static void
DrawLine(float x1, float y1, float x2, float y2, const CRGBA &col)
{
	float dx = x2 - x1, dy = y2 - y1;
	int steps = (int)(Max(Abs(dx), Abs(dy)) / 2.0f) + 1;
	float t = SCREEN_SCALE_Y(1.0f);
	for (int i = 0; i <= steps; i++) {
		float x = x1 + dx * i / steps, y = y1 + dy * i / steps;
		CSprite2d::DrawRect(CRect(x - t, y - t, x + t, y + t), col);
	}
}

void
DrawPadPicture(void)
{
	LoadPadSprite();
	if (!padSprite->m_pTexture && !padPrompted && !revc_import_busy() && !revc_import_status(0)) {
		padPrompted = true;	// ask for the Xbox disc once a session
		revc_import_prompt();
	}

	// The outline: 360x180 of the 640x448 menu, centred above the entries.
	const float px = 140.0f, py = 105.0f, pw = 360.0f, ph = 180.0f;
	CRect padRect(MENU_X_LEFT_ALIGNED(px), MENU_Y(py), MENU_X_LEFT_ALIGNED(px + pw), MENU_Y(py + ph));
	if (padSprite->m_pTexture)
		padSprite->Draw(padRect, CRGBA(255, 255, 255, FrontEndMenuManager.FadeIn(255)));
	else {
		CFont::SetFontStyle(FONT_LOCALE(FONT_STANDARD));
		CFont::SetScale(MENU_X(0.35f), MENU_Y(0.7f));
		CFont::SetCentreOn();
		CFont::SetCentreSize(MENU_X(pw));
		CFont::SetColor(CRGBA(255, 255, 255, FrontEndMenuManager.FadeIn(255)));
		CFont::PrintString(MENU_X_LEFT_ALIGNED(px + pw / 2), MENU_Y(py + ph / 2 - 10), TheText.Get("RVC_PPI"));
	}

	// Which layout this is.
	int layout = PadLayoutCurrent();
	CFont::SetFontStyle(FONT_LOCALE(FONT_HEADING));
	CFont::SetScale(MENU_X(0.6f), MENU_Y(0.9f));
	CFont::SetCentreOn();
	CFont::SetCentreSize(SCREEN_WIDTH);
	CFont::SetColor(CRGBA(MENUOPTION_COLOR.r, MENUOPTION_COLOR.g, MENUOPTION_COLOR.b, FrontEndMenuManager.FadeIn(255)));
	// the Xbox skin's title runs long at the top left: the layout name goes under it
	extern bool MenuSkinXboxActive(int page, bool gameNotLoaded);
	bool xbox = MenuSkinXboxActive(FrontEndMenuManager.m_nCurrScreen, FrontEndMenuManager.m_bGameNotLoaded);
	CFont::PrintString(MENU_X_LEFT_ALIGNED(320), MENU_Y(xbox ? 86 : 72),
		TheText.Get(layout == 0 ? "RVC_PLS" : layout == 1 ? "RVC_PLC" : layout == 2 ? "RVC_PLO" : "RVC_PLX"));

	// One callout per control that does something in this context.
	Callout callouts[ARRAY_SIZE(padSpots)];
	int n = 0;
	for (int s = 0; s < ARRAY_SIZE(padSpots); s++) {
		Callout &c = callouts[n];
		c.spot = &padSpots[s];
		c.text[0] = 0;
		if (c.spot->joy == 20)
			AppendText(c.text, padContext == 0 ? "RVC_MOV" : "RVC_STR", ARRAY_SIZE(c.text));
		else if (c.spot->joy == 21)
			AppendText(c.text, "RVC_CAM", ARRAY_SIZE(c.text));
		else
			for (int a = 0; a < ARRAY_SIZE(padActions); a++) {
				const PadAction &pa = padActions[a];
				if ((pa.contexts & (1 << padContext)) &&
				    ControlsManager.GetControllerKeyAssociatedWithAction(pa.action, JOYSTICK) == c.spot->joy)
					AppendText(c.text, pa.text, ARRAY_SIZE(c.text));
			}
		if (c.text[0])
			n++;
	}

	// Left half of the pad -> labels on the left, right half -> on the right, each side in the
	// order of the controls from top to bottom.
	CFont::SetFontStyle(FONT_LOCALE(FONT_STANDARD));
	CFont::SetScale(MENU_X(0.38f), MENU_Y(0.76f));
	CFont::SetCentreOff();
	CFont::SetPropOn();
	CRGBA lineCol(255, 150, 225, FrontEndMenuManager.FadeIn(200));
	// Xbox skin: the Xbox page's look, teal lines and green labels in its menu font (measured)
	extern bool MenuSkinXboxText(float x, float y, float height, const wchar *text, const CRGBA &col, int align);
	if (xbox)
		lineCol = CRGBA(26, 165, 120, FrontEndMenuManager.FadeIn(230));
	for (int side = 0; side < 2; side++) {
		const Callout *list[ARRAY_SIZE(padSpots)];
		int m = 0;
		for (int i = 0; i < n; i++)
			if ((callouts[i].spot->x >= 256.0f) == (side == 1))
				list[m++] = &callouts[i];
		for (int i = 1; i < m; i++)	// by height on the pad
			for (int j = i; j > 0 && list[j]->spot->y < list[j - 1]->spot->y; j--) {
				const Callout *t = list[j]; list[j] = list[j - 1]; list[j - 1] = t;
			}
		const float top = 100.0f, bottom = 300.0f;
		for (int i = 0; i < m; i++) {
			float ly = m == 1 ? (top + bottom) / 2 : top + (bottom - top) * i / (m - 1);
			float ax = px + list[i]->spot->x * pw / 512.0f, ay = py + list[i]->spot->y * ph / 256.0f;
			float lx = side == 0 ? 132.0f : 508.0f;
			static wchar line[128];
			char glyph[8];
			snprintf(glyph, sizeof(glyph), list[i]->spot->glyph[0] ? "%s " : "%s", list[i]->spot->glyph);
			AsciiToUnicode(glyph, line);
			UnicodeStrcpy(line + UnicodeStrlen(line), list[i]->text);
			CFont::SetColor(CRGBA(255, 255, 255, FrontEndMenuManager.FadeIn(255)));
			if (side == 0) {
				CFont::SetRightJustifyOn();
				CFont::SetRightJustifyWrap(0.0f);
			} else {
				CFont::SetRightJustifyOff();
				CFont::SetWrapx(SCREEN_WIDTH);
			}
			if (!(xbox && MenuSkinXboxText(MENU_X_LEFT_ALIGNED(lx), MENU_Y(ly - 9.0f), MENU_Y(18.0f), list[i]->text,
				CRGBA(40, 205, 120, FrontEndMenuManager.FadeIn(255)), side == 0 ? 1 : 0)))
				CFont::PrintString(MENU_X_LEFT_ALIGNED(lx), MENU_Y(ly - 6.0f), line);
			DrawLine(MENU_X_LEFT_ALIGNED(lx + (side == 0 ? 4.0f : -4.0f)), MENU_Y(ly),
				MENU_X_LEFT_ALIGNED(ax), MENU_Y(ay), lineCol);
			float d = SCREEN_SCALE_Y(2.5f);
			CSprite2d::DrawRect(CRect(MENU_X_LEFT_ALIGNED(ax) - d, MENU_Y(ay) - d, MENU_X_LEFT_ALIGNED(ax) + d, MENU_Y(ay) + d), lineCol);
		}
	}
	CFont::SetRightJustifyOff();
	CFont::DrawFonts();
}

// revc (plan task 13): the PS2 Controls page's DualShock 2 picture (fe_controller from the PS2's
// frontend2 TXD, here from a texture pack: the game can't read PS2 TXDs yet) with the PS2's
// callouts: black lines with a dot at the label end, plain white labels in two columns. The labels
// follow the current bindings (revc's layouts aren't the PS2's setups 1-4), like the Xbox page.
// Positions measured on the PS2 captures at 640x448; spots in the 1024 px texture.
extern "C" int revc_texmod_find(const char *txd, const char *texture, int *width, int *height, unsigned char **rgba);
static CSprite2d *ds2Sprite;
static const PadSpot ds2Spots[] = {
	{ 4, 796, 366, "" }, { 1, 860, 430, "" }, { 3, 730, 430, "" }, { 2, 796, 496, "" },
	{ 5, 224, 156, "" }, { 6, 794, 156, "" }, { 7, 224, 210, "" }, { 8, 794, 210, "" },
	{ 9, 420, 430, "" }, { 12, 594, 430, "" }, { 10, 370, 600, "" }, { 11, 640, 600, "" },
	{ 13, 224, 370, "" }, { 14, 276, 430, "" }, { 15, 224, 480, "" }, { 16, 170, 430, "" },
	{ 20, 370, 560, "" }, { 21, 640, 560, "" },
};

void
DrawPadPicturePS2(void)
{
	if (ds2Sprite == nil)
		ds2Sprite = new CSprite2d;
	if (ds2Sprite->m_pTexture == nil) {
		int w, h;
		unsigned char *px;
		if (!revc_texmod_find("frontend2", "fe_controller", &w, &h, &px))
			return;
		rw::Image *img = rw::Image::create(w, h, 32);
		img->pixels = px;
		img->stride = w * 4;
		rw::Raster *ras = rw::Raster::create(w, h, 32, rw::Raster::C8888 | rw::Raster::TEXTURE | rw::Raster::MIPMAP | rw::Raster::AUTOMIPMAP);
		if (ras && ras->setFromImage(img)) {
			rw::Texture *tex = rw::Texture::create(ras);
			tex->setFilter(rw::Texture::LINEARMIPLINEAR);
			ds2Sprite->m_pTexture = tex;
		} else if (ras)
			ras->destroy();
		img->pixels = nil;
		img->destroy();
		revc_texmod_free(px, w, h);
		if (ds2Sprite->m_pTexture == nil)
			return;
	}
	const float px = 160.0f, py = 168.0f, pw = 239.0f, ph = 195.0f;
	uint8 alpha = FrontEndMenuManager.FadeIn(255);
	ds2Sprite->Draw(CRect(MENU_X_LEFT_ALIGNED(px), MENU_Y(py), MENU_X_LEFT_ALIGNED(px + pw), MENU_Y(py + ph)), CRGBA(255, 255, 255, alpha));

	Callout callouts[ARRAY_SIZE(ds2Spots)];
	int n = 0;
	for (int s = 0; s < ARRAY_SIZE(ds2Spots); s++) {
		Callout &c = callouts[n];
		c.spot = &ds2Spots[s];
		c.text[0] = 0;
		if (c.spot->joy == 20)
			AppendText(c.text, padContext == 0 ? "RVC_MOV" : "RVC_STR", ARRAY_SIZE(c.text));
		else if (c.spot->joy == 21)
			AppendText(c.text, "RVC_CAM", ARRAY_SIZE(c.text));
		else
			for (int a = 0; a < ARRAY_SIZE(padActions); a++) {
				const PadAction &pa = padActions[a];
				if ((pa.contexts & (1 << padContext)) &&
				    ControlsManager.GetControllerKeyAssociatedWithAction(pa.action, JOYSTICK) == c.spot->joy)
					AppendText(c.text, pa.text, ARRAY_SIZE(c.text));
			}
		if (c.text[0])
			n++;
	}
	CFont::SetFontStyle(FONT_LOCALE(FONT_STANDARD));
	CFont::SetScale(MENU_X(0.32f), MENU_Y(0.62f));
	CFont::SetCentreOff();
	CFont::SetPropOn();
	CFont::SetDropShadowPosition(0);
	CFont::SetColor(CRGBA(255, 255, 255, alpha));
	CRGBA black(0, 0, 0, alpha);
	for (int side = 0; side < 2; side++) {
		const Callout *list[ARRAY_SIZE(ds2Spots)];
		int m = 0;
		for (int i = 0; i < n; i++)
			if ((callouts[i].spot->x >= 512.0f) == (side == 1))
				list[m++] = &callouts[i];
		for (int i = 1; i < m; i++)
			for (int j = i; j > 0 && list[j]->spot->y < list[j - 1]->spot->y; j--) {
				const Callout *t = list[j]; list[j] = list[j - 1]; list[j - 1] = t;
			}
		// the right column ends above the help box (console)
		const float top = 172.0f, bottom = side == 0 ? 340.0f : 290.0f;
		for (int i = 0; i < m; i++) {
			float ly = m == 1 ? (top + bottom) / 2 : top + (bottom - top) * i / (m - 1);
			float ax = px + list[i]->spot->x * pw / 1024.0f, ay = py + list[i]->spot->y * ph / 1024.0f;
			float lx = side == 0 ? 152.0f : 404.0f;
			if (side == 0) {
				CFont::SetRightJustifyOn();
				CFont::SetRightJustifyWrap(0.0f);
			} else {
				CFont::SetRightJustifyOff();
				CFont::SetWrapx(SCREEN_WIDTH);
			}
			CFont::PrintString(MENU_X_LEFT_ALIGNED(lx), MENU_Y(ly - 6.0f), (wchar *)list[i]->text);
			float sx = lx + (side == 0 ? 4.0f : -4.0f);
			DrawLine(MENU_X_LEFT_ALIGNED(sx), MENU_Y(ly), MENU_X_LEFT_ALIGNED(ax), MENU_Y(ay), black);
			float d = SCREEN_SCALE_Y(2.5f);
			CSprite2d::DrawRect(CRect(MENU_X_LEFT_ALIGNED(sx) - d, MENU_Y(ly) - d, MENU_X_LEFT_ALIGNED(sx) + d, MENU_Y(ly) + d), black);
		}
	}
	CFont::SetRightJustifyOff();
	CFont::DrawFonts();
}
