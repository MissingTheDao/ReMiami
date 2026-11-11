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

static int8 padContext;	// 0 on foot, 1 in car
// Heap, never destroyed: a static CSprite2d would free its texture after RenderWare shut down.
static CSprite2d *padSprite;
static bool padPrompted;

wchar*
PadPictureContextDraw(bool *disabled, bool userHovering)
{
	return TheText.Get(padContext == 0 ? "RVC_PPF" : "RVC_PPC");
}

void
PadPictureContextPress(int8 action)
{
	if (action == FEOPTION_ACTION_LEFT || action == FEOPTION_ACTION_RIGHT || action == FEOPTION_ACTION_SELECT)
		padContext ^= 1;
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
struct PadAction { e_ControllerAction action; const char *text; int8 context; };	// context: 0 foot, 1 car, 2 both
static const PadAction padActions[] = {
	{ PED_FIREWEAPON, "FEC_FIR", 0 }, { PED_LOCK_TARGET, "FEC_TAR", 0 }, { PED_SPRINT, "FEC_SPN", 0 },
	{ PED_JUMPING, "FEC_JMP", 0 }, { VEHICLE_ENTER_EXIT, "FEC_EEX", 2 }, { PED_CYCLE_WEAPON_RIGHT, "FEC_NWE", 0 },
	{ PED_CYCLE_WEAPON_LEFT, "FEC_PWE", 0 }, { PED_ANSWER_PHONE, "FEC_ANS", 0 }, { PED_DUCK, "FEC_CRO", 0 },
	{ PED_LOOKBEHIND, "FEC_LBA", 0 }, { CAMERA_CHANGE_VIEW_ALL_SITUATIONS, "FEC_CMR", 2 },
	{ PED_CENTER_CAMERA_BEHIND_PLAYER, "FEC_CEN", 0 }, { PED_CYCLE_TARGET_RIGHT, "FEC_NTR", 0 },
	{ PED_CYCLE_TARGET_LEFT, "FEC_PTT", 0 }, { GO_FORWARD, "FEC_FOR", 0 }, { GO_BACK, "FEC_BAC", 0 },
	{ GO_LEFT, "FEC_LEF", 0 }, { GO_RIGHT, "FEC_RIG", 0 },
	{ VEHICLE_ACCELERATE, "RVC_ACC", 1 }, { VEHICLE_BRAKE, "RVC_BRK", 1 }, { VEHICLE_HANDBRAKE, "FEC_HND", 1 },
	{ VEHICLE_FIREWEAPON, "FEC_FIR", 1 }, { VEHICLE_HORN, "FEC_HRN", 1 }, { TOGGLE_SUBMISSIONS, "FEC_SUB", 1 },
	{ VEHICLE_CHANGE_RADIO_STATION, "FEC_RAD", 1 }, { VEHICLE_LOOKLEFT, "FEC_LOL", 1 }, { VEHICLE_LOOKRIGHT, "FEC_LOR", 1 },
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
				if ((pa.context == padContext || pa.context == 2) &&
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
