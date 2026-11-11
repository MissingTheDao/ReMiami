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
bool Ps2SetupActive(void);	// (ControllerConfig.cpp)

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

// The PS2's own labels for its setups 1-4 (the PS2 UI style with a setup chosen), on foot and in
// car, read off the v3.00 captures. Button numbers as the spots: 1 circle, 2 cross, 3 square,
// 4 triangle, 5 L2, 6 R2, 7 L1, 8 R1, 9 select, 10 L3, 11 R3, 12 start, 13 D-pad, 20/21 the sticks.
struct Ps2Label { int joy; const char *text; };
static const Ps2Label ps2FootCommon[] = {
	{ 5, "Cycle Weapon left" }, { 6, "Cycle Weapon right" }, { 3, "Jump" }, { 20, "Movement" },
	{ 10, "Crouch [L3 button]" }, { 21, "First person camera" }, { 11, "Look behind [R3 button]" }, { 12, "Pause" },
};
static const Ps2Label ps2Foot[4][8] = {
	{ { 7, "Look forward" }, { 8, "Target" }, { 4, "Enter vehicle" }, { 1, "Attack or Fire weapon" }, { 2, "Run" }, { 13, "Movement" }, { 9, "Camera modes" } },
	{ { 7, "Look forward" }, { 8, "Target" }, { 4, "Enter vehicle" }, { 1, "Attack or Fire weapon" }, { 2, "Run" }, { 13, "Camera modes" }, { 9, "NA" } },
	{ { 7, "Enter vehicle" }, { 8, "Target" }, { 4, "Look forward" }, { 1, "Run" }, { 2, "Attack or Fire weapon" }, { 13, "Movement" }, { 9, "Camera modes" } },
	{ { 7, "Target" }, { 8, "Attack or Fire weapon" }, { 4, "Enter vehicle" }, { 1, "Look forward" }, { 2, "Run" }, { 13, "NA" }, { 9, "Camera modes" } },
};
static const Ps2Label ps2CarCommon[] = {
	{ 5, "Look left" }, { 6, "Look right" }, { 22, "Look behind" }, { 12, "Pause" },	// (22: the L2+R2 bracket)
};
static const Ps2Label ps2Car[4][12] = {
	{ { 7, "Radio stations" }, { 8, "Hand brake" }, { 4, "Enter and exit vehicle" }, { 1, "Fire" }, { 2, "Accelerate" }, { 3, "Brake or Reverse" },
	  { 13, "Vehicle control" }, { 20, "Vehicle control" }, { 10, "Horn [L3 button]" }, { 21, "Turret control" }, { 11, "Sub-mission [R3 button]" }, { 9, "Camera modes" } },
	{ { 7, "Horn" }, { 8, "Hand brake" }, { 4, "Enter and exit vehicle" }, { 1, "Fire" }, { 2, "Accelerate" }, { 3, "Brake or Reverse" },
	  { 13, "Camera modes" }, { 20, "Vehicle control" }, { 10, "NA" }, { 21, "Turret control" }, { 11, "Sub-mission [R3 button]" }, { 9, "Radio stations" } },
	{ { 7, "Enter and exit vehicle" }, { 8, "Horn" }, { 4, "Hand brake" }, { 1, "Fire" }, { 2, "Accelerate" }, { 3, "Brake or Reverse" },
	  { 13, "Vehicle control" }, { 20, "Vehicle control" }, { 10, "Radio station cycle [L3 button]" }, { 21, "Turret control" }, { 11, "Sub-mission [R3 button]" }, { 9, "Camera modes" } },
	{ { 7, "Hand brake" }, { 8, "Fire" }, { 4, "Enter and exit vehicle" }, { 1, "Radio stations" }, { 2, "NA" }, { 3, "Sub-mission" },
	  { 13, "Turret control" }, { 20, "Vehicle control" }, { 10, "Horn [L3 button]" }, { 21, "Accelerate, Brake or Reverse" }, { 9, "Camera modes" } },
};

// The label of one PS2 spot for the current PS2 setup (false: none, or not in a setup).
static bool
Ps2SetupLabel(int joy, int context, wchar *out)
{
	extern int8 gPs2Setup;
	if (!Ps2SetupActive() || context > 1)
		return false;
	const Ps2Label *common = context == 0 ? ps2FootCommon : ps2CarCommon;
	int nc = context == 0 ? ARRAY_SIZE(ps2FootCommon) : ARRAY_SIZE(ps2CarCommon);
	const Ps2Label *own = context == 0 ? ps2Foot[gPs2Setup] : ps2Car[gPs2Setup];
	int no = context == 0 ? 8 : 12;
	const char *t = nil;
	for (int i = 0; i < nc && !t; i++)
		if (common[i].joy == joy) t = common[i].text;
	for (int i = 0; i < no && !t; i++)
		if (own[i].text && own[i].joy == joy) t = own[i].text;
	if (t == nil)
		return false;
	char buf[96];
	strncpy(buf, t, sizeof(buf) - 1);
	buf[sizeof(buf) - 1] = 0;
	AsciiToUnicode(buf, out);
	return true;
}

// The PS2's own callout overlay (fe_arrows1-4 from FRONTEN2): each a whole overlay drawn over
// exactly the pad's rectangle (re3's Frontend_PS2.cpp: same AddPicture rect as fe_controller);
// arrows1 on foot / arrows2 in car, arrows3 / arrows4 for setup 2. Each label where the console
// puts it at its arrow's end (read off the v3.00 captures): align 0 right-aligned to x,
// 1 left-aligned, 2 centred; wrap: the width the console breaks lines at.
struct Ps2Anchor { int joy; float x, y; int align; float wrap; };
static const Ps2Anchor ps2FootAnchors[] = {
	{ 5, 171, 159, 2, 200 }, { 6, 403, 159, 2, 200 }, { 7, 150, 198, 0, 70 }, { 3, 315, 191, 0, 80 },
	{ 8, 404, 201, 1, 220 }, { 4, 404, 228, 1, 220 }, { 13, 151, 241, 0, 80 }, { 1, 404, 241, 1, 220 },
	{ 2, 404, 254, 1, 220 }, { 20, 151, 266, 0, 80 }, { 21, 404, 271, 1, 220 }, { 10, 151, 281, 0, 60 },
	{ 11, 404, 284, 1, 136 }, { 12, 314, 303, 2, 80 }, { 9, 271, 316, 2, 130 },
};
static const Ps2Anchor ps2CarAnchors[] = {
	{ 5, 210, 159, 2, 120 }, { 6, 365, 159, 2, 120 }, { 22, 279, 169, 2, 120 }, { 3, 282, 183, 2, 66 },
	{ 7, 150, 198, 0, 70 }, { 8, 404, 201, 1, 220 }, { 4, 404, 228, 1, 220 }, { 13, 150, 236, 0, 70 },
	{ 1, 404, 241, 1, 220 }, { 2, 404, 254, 1, 220 }, { 20, 150, 265, 0, 70 }, { 21, 404, 271, 1, 220 },
	{ 11, 404, 284, 1, 220 }, { 12, 314, 303, 2, 80 }, { 9, 296, 315, 2, 70 }, { 10, 185, 335, 2, 160 },
};
static CSprite2d *ps2Arrows;

static bool
LoadPackSprite(CSprite2d &spr, const char *texture)
{
	if (spr.m_pTexture)
		return true;
	int w, h;
	unsigned char *px;
	if (!revc_texmod_find("frontend2", texture, &w, &h, &px))
		return false;
	rw::Image *img = rw::Image::create(w, h, 32);
	img->pixels = px;
	img->stride = w * 4;
	rw::Raster *ras = rw::Raster::create(w, h, 32, rw::Raster::C8888 | rw::Raster::TEXTURE | rw::Raster::MIPMAP | rw::Raster::AUTOMIPMAP);
	if (ras && ras->setFromImage(img)) {
		rw::Texture *tex = rw::Texture::create(ras);
		tex->setFilter(rw::Texture::LINEARMIPLINEAR);
		spr.m_pTexture = tex;
	} else if (ras)
		ras->destroy();
	img->pixels = nil;
	img->destroy();
	revc_texmod_free(px, w, h);
	return spr.m_pTexture != nil;
}

// The PS2 setups' page: overlay and labels; false without the overlay textures.
static bool
DrawPs2SetupCallouts(int context, uint8 alpha, const CRect &pad)
{
	extern int8 gPs2Setup;
	if (ps2Arrows == nil)
		ps2Arrows = new CSprite2d[4];
	const bool car = context == 1;
	int v = (car ? 1 : 0) + (gPs2Setup == 1 ? 2 : 0);	// 0..3: fe_arrows1..4
	static const char *names[4] = { "fe_arrows1", "fe_arrows2", "fe_arrows3", "fe_arrows4" };
	if (!LoadPackSprite(ps2Arrows[v], names[v]))
		return false;
	ps2Arrows[v].Draw(pad, CRGBA(255, 255, 255, alpha));
	const Ps2Anchor *anchors = car ? ps2CarAnchors : ps2FootAnchors;
	int n = car ? ARRAY_SIZE(ps2CarAnchors) : ARRAY_SIZE(ps2FootAnchors);
	CFont::SetFontStyle(FONT_LOCALE(FONT_STANDARD));
	CFont::SetScale(MENU_X(0.4564f), MENU_Y(0.63f));	// re3's CONTR_DESCR_NEW_TEXTSCALE (the PS2's)
	CFont::SetPropOn();
	CFont::SetDropShadowPosition(1);
	CFont::SetDropColor(CRGBA(0, 0, 0, alpha));
	CFont::SetColor(CRGBA(255, 255, 255, alpha));
	for (int i = 0; i < n; i++) {
		static wchar text[96];
		text[0] = 0;
		if (!Ps2SetupLabel(anchors[i].joy, context, text) || text[0] == 0)
			continue;
		const Ps2Anchor &a = anchors[i];
		CFont::SetCentreOff();
		CFont::SetRightJustifyOff();
		if (a.align == 0) {
			CFont::SetRightJustifyOn();
			CFont::SetRightJustifyWrap(MENU_X_LEFT_ALIGNED(a.x - a.wrap));
		} else if (a.align == 1)
			CFont::SetWrapx(MENU_X_LEFT_ALIGNED(a.x + a.wrap));
		else {
			CFont::SetCentreOn();
			CFont::SetCentreSize(MENU_X(a.wrap));
		}
		CFont::PrintString(MENU_X_LEFT_ALIGNED(a.x), MENU_Y(a.y), text);
	}
	CFont::SetCentreOff();
	CFont::SetRightJustifyOff();
	CFont::SetWrapx(SCREEN_WIDTH);
	CFont::SetDropShadowPosition(0);
	CFont::DrawFonts();
	return true;
}

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
	if (Ps2SetupActive() && DrawPs2SetupCallouts(padContext, alpha,
	        CRect(MENU_X_LEFT_ALIGNED(px), MENU_Y(py), MENU_X_LEFT_ALIGNED(px + pw), MENU_Y(py + ph))))
		return;	// the PS2's own overlay and labels

	Callout callouts[ARRAY_SIZE(ds2Spots)];
	int n = 0;
	for (int s = 0; s < ARRAY_SIZE(ds2Spots); s++) {
		Callout &c = callouts[n];
		c.spot = &ds2Spots[s];
		c.text[0] = 0;
		if (Ps2SetupLabel(c.spot->joy, padContext, c.text))
			;	// the PS2's own label for its setup
		else if (Ps2SetupActive())
			;	// the setup gives this button no label
		else if (c.spot->joy == 20)
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
