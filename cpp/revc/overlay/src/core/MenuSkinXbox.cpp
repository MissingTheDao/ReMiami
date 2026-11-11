// revc (plan task 13h): Xbox skin for the PC menu, with UI STYLE = Xbox.
//
// As the PS2 skin (MenuSkinPS2.cpp): the menu logic, pages and options stay the PC menu's; this
// draws the Xbox pause menu's look: the page title top left and the Vice City logo top right, the
// row of page icons at the bottom (the current one enlarged on a mint box), the pink help line
// under it, items in two columns with the mint selection bar. The sprites are the Xbox's own
// (neo/UserInterface, imported from the player's disc); without an import the PC look stays.
// Positions are measured on native 640x480 xemu captures (docs/CONSOLE_STYLES.md 13h) and given
// in those pixels; the Xbox UI style lays out in 480 lines.
#include "common.h"
#include "Frontend.h"
#include "Font.h"
#include "Sprite2d.h"
#include "Text.h"
#include "Timer.h"
#include "Pad.h"

extern int8 gUiStyle;
extern "C" int revc_import_rgba(int kind, const char *rel, int *width, int *height, unsigned char **rgba);
extern "C" void revc_texmod_free(unsigned char *rgba, int width, int height);
extern "C" ssize_t revc_import_read(int kind, const char *rel, uint8 *buf, size_t cap);

static float X(float x) { return SCALE_AND_CENTER_X(x); }
static float Y(float y) { return SCREEN_SCALE_Y(y); }
static float W(float w) { return SCREEN_SCALE_X(w); }
static float H(float h) { return SCREEN_SCALE_Y(h); }

static bool HasSubTabs(int page);
void ConsoleAudioRows(bool);	// (core/MenuScreensCustom.cpp)
static bool SettingsSubTabs(int page);
const CRGBA XBOX_MINT(0, 210, 141, 255);
const CRGBA XBOX_PINK(255, 154, 227, 255);

// The Xbox pause menu's seven pages in icon order, the PC page shown for each, the icon sprite.
struct XboxTab { int page; int alt; const char *icon; };
static const XboxTab tabs[] = {
	{ MENUPAGE_NEW_GAME, MENUPAGE_CHOOSE_LOAD_SLOT, "ui/r3_menu_238.dds" },	// disk: load ("new game / load game", then the slots)
	{ MENUPAGE_MAP, -1, "ui/r3_menu_241.dds" },			// compass
	{ MENUPAGE_BRIEFS, -1, "ui/r3_menu_243.dds" },		// tape
	{ MENUPAGE_STATS, -1, "ui/r3_menu_246.dds" },			// computer
	{ MENUPAGE_CONSOLE_CONTROLS, MENUPAGE_PAD_PICTURE, "ui/r3_menu_249.dds" },	// pad: the console's controls page (and the controller picture)
	{ MENUPAGE_SOUND_SETTINGS, -1, "ui/r3_menu_252.dds" },	// boombox
	{ MENUPAGE_CONSOLE_DISPLAY, -1, "ui/r3_menu_255.dds" },	// TV: the console's display page
};
// icon centres on the bar (measured)
static const float iconX[7] = { 112.0f, 180.0f, 253.0f, 325.0f, 395.0f, 463.0f, 530.0f };
static const float ICON_Y = 395.0f, ICON_SIZE = 50.0f, ICON_SELECTED = 80.0f;

// Heap-allocated so nothing runs at program exit (after RenderWare is gone); freed with the menu's
// own textures by MenuSkinXboxUnload.
static CSprite2d *icons, *logoSprite, *glint, *tapeDeck;
// Focus on the icon bar (the Xbox's top level): after LT/RT until the player moves in the page's
// list. The current icon's green card and the help line's items follow it.
static bool barFocus;
static int barFocusOption = -1;
#define logo (*logoSprite)
static int spritesState;	// 0 not tried, 1 loaded, -1 no import

static rw::Texture *
ImportTexture(const char *rel)
{
	int w, h;
	unsigned char *px;
	if (!revc_import_rgba(0, rel, &w, &h, &px))
		return nil;
	rw::Image *img = rw::Image::create(w, h, 32);
	img->pixels = px;
	img->stride = w * 4;
	rw::Raster *ras = rw::Raster::create(w, h, 32, rw::Raster::C8888 | rw::Raster::TEXTURE | rw::Raster::MIPMAP | rw::Raster::AUTOMIPMAP);
	rw::Texture *tex = nil;
	if (ras && ras->setFromImage(img)) {
		tex = rw::Texture::create(ras);
		tex->setFilter(rw::Texture::LINEARMIPLINEAR);
	} else if (ras)
		ras->destroy();
	img->pixels = nil;
	img->destroy();
	revc_texmod_free(px, w, h);
	return tex;
}

// The Xbox menu's bitmap fonts (neo/UserInterface/fonts: RWFont, RWSFArborcrestHeavy): a 256 px
// texture and a METRICS1 file with each character's box (code, left, top, right, bottom) in those
// 256 px, so a higher-resolution remake works as fractions of the texture.
struct XboxFont {
	CSprite2d *sprite;
	struct Box { int16 l, t, r, b; } box[256];
	bool ok;
};
static XboxFont fontRW;

static bool
LoadFont(XboxFont &f, const char *tex, const char *met)
{
	f.ok = false;
	if (f.sprite == nil)
		f.sprite = new CSprite2d;
	f.sprite->m_pTexture = ImportTexture(tex);
	static uint8 buf[16384];
	ssize_t len = revc_import_read(0, met, buf, sizeof(buf) - 1);
	if (f.sprite->m_pTexture == nil || len <= 0)
		return false;
	buf[len] = 0;
	memset(f.box, 0, sizeof(f.box));
	char *p = (char *)buf;
	for (int line = 0; *p; line++) {
		char *end = strchr(p, '\n');
		if (end)
			*end = 0;
		int c, l, t, r, b;
		if (line >= 3 && sscanf(p, "%d %d %d %d %d", &c, &l, &t, &r, &b) == 5 && c >= 0 && c < 256)
			f.box[c] = { (int16)l, (int16)t, (int16)r, (int16)b };
		if (!end)
			break;
		p = end + 1;
	}
	f.ok = true;
	return true;
}

// Width of `text` at `height` px (the font's 28 px cells scaled).
static float
XboxFontWidth(const XboxFont &f, const wchar *text, float height)
{
	float w = 0.0f, s = height / 28.0f;
	for (; *text; text++) {
		const XboxFont::Box &b = f.box[*text < 256 ? *text : '?'];
		w += (b.r - b.l + 1) * s;
	}
	return w;
}

static void
XboxFontPrint(const XboxFont &f, float x, float y, float height, const wchar *text, const CRGBA &col)
{
	if (!f.ok)
		return;
	float s = height / 28.0f;
	for (; *text; text++) {
		const XboxFont::Box &b = f.box[*text < 256 ? *text : '?'];
		float w = (b.r - b.l + 1) * s;
		if (b.r > b.l && *text != ' ') {
			float u0 = b.l / 256.0f, u1 = (b.r + 1) / 256.0f, v0 = b.t / 256.0f, v1 = (b.b + 1) / 256.0f;
			f.sprite->Draw(CRect(x, y, x + w, y + (b.b - b.t + 1) * s), col, u0, v0, u1, v0, u0, v1, u1, v1);
		}
		x += w;
	}
}

// Text in the Xbox menu font for other menu code (the controller page's callouts): align 0 left,
// 1 right of x; a dark drop shadow. False when the font isn't loaded (then draw as before).
bool
MenuSkinXboxText(float x, float y, float height, const wchar *text, const CRGBA &col, int align)
{
	if (!fontRW.ok)
		return false;
	if (align == 1)
		x -= XboxFontWidth(fontRW, text, height);
	float d = height / 10.0f;
	XboxFontPrint(fontRW, x + d, y + d, height, text, CRGBA(0, 0, 0, col.a));
	XboxFontPrint(fontRW, x, y, height, text, col);
	return true;
}

float
MenuSkinXboxTextWidth(const wchar *text, float height)
{
	return fontRW.ok ? XboxFontWidth(fontRW, text, height) : 0.0f;
}

static bool
LoadSprites(void)
{
	if (spritesState == 0) {
		spritesState = -1;
		if (icons == nil) {
			icons = new CSprite2d[7];
			logoSprite = new CSprite2d;
			glint = new CSprite2d[2];
			tapeDeck = new CSprite2d;
		}
		bool ok = true;
		for (int i = 0; i < 7; i++)
			ok = ok && (icons[i].m_pTexture = ImportTexture(tabs[i].icon)) != nil;
		logo.m_pTexture = ImportTexture("ui/r3_menu_228.dds");
		glint[0].m_pTexture = ImportTexture("ui/r3_menu_230.dds");
		glint[1].m_pTexture = ImportTexture("ui/r3_menu_232.dds");
		tapeDeck->m_pTexture = ImportTexture("ui/r3_menu_616.dds");	// the user's music in the radio strip
		LoadFont(fontRW, "ui/fonts/rwsfarborcrestheavy.dds", "ui/fonts/rwsfarborcrestheavy.met");	// the help line's font
		if (ok && logo.m_pTexture)
			spritesState = 1;
	}
	return spritesState == 1;
}

// Called from CMenuManager::UnloadTextures: frees the sprites; they load again when needed.
void
MenuSkinXboxUnload(void)
{
	if (icons == nil)
		return;
	for (int i = 0; i < 7; i++)
		icons[i].Delete();
	logo.Delete();
	glint[0].Delete();
	glint[1].Delete();
	tapeDeck->Delete();
	if (fontRW.sprite)
		fontRW.sprite->Delete();
	fontRW.ok = false;
	spritesState = 0;
}

// The radio strip's last icon, the user's music: the Xbox's stack of tapes (r3_menu_616). False
// without it (the caller draws the PC's MP3 logo).
bool
MenuSkinXboxTapeDeck(const CRect &r, const CRGBA &col)
{
	if (tapeDeck == nil || tapeDeck->m_pTexture == nil)
		return false;
	tapeDeck->Draw(r, col);
	return true;
}

// The confirmation dialogs reached from the Load page: drawn as the Xbox's layered blue dialog
// over the dimmed Load page.
bool
MenuSkinXboxDialog(int page)
{
	return page == MENUPAGE_NEW_GAME_RELOAD || page == MENUPAGE_LOAD_SLOT_CONFIRM || page == MENUPAGE_DELETE_SLOT_CONFIRM;
}

// Behind a dialog the console dims the title, logo, icon bar and help line too (to about 45 %).
static uint8
DialogDim(uint8 alpha)
{
	return MenuSkinXboxDialog(FrontEndMenuManager.m_nCurrScreen) ? alpha * 45 / 100 : alpha;
}

// The "advanced" sub-tab (MENUPAGE_CONSOLE_OPTIONS) belongs to the page it was opened from.
static int optionsOrigin = MENUPAGE_CONSOLE_DISPLAY;
// Focus on the sub-tab bar of Display / Audio / Controls (down from their last row; up leaves).
static bool subBarFocus;

static int
CurrentTab(int page)
{
	if (MenuSkinXboxDialog(page))
		return 0;
	if (page == MENUPAGE_CONSOLE_OPTIONS)
		page = optionsOrigin;
	for (int i = 0; i < (int)ARRAY_SIZE(tabs); i++)
		if (tabs[i].page == page || tabs[i].alt == page)
			return i;
	return -1;
}

// The Xbox page (icon) a menu page belongs to, -1 if none.
int
MenuSkinXboxTab(int page)
{
	return CurrentTab(page);
}

// Whether the skin draws on this page: Xbox UI style, a game running, a page the Xbox pause menu
// has, and the Xbox menu sprites imported.
bool
MenuSkinXboxActive(int page, bool gameNotLoaded)
{
	static bool test = getenv("REVC_TEST_MENU_SKIN") != nil;
	return gUiStyle == 2 && (!gameNotLoaded || test) && CurrentTab(page) >= 0 && LoadSprites();
}

// The page title, top left in the menu's heading font with its dark shadow (instead of the PC's
// top right).
void
MenuSkinXboxTitle(wchar *title, uint8 alpha)
{
	// the Xbox's own names for these pages
	int page = FrontEndMenuManager.m_nCurrScreen;
	if (page == MENUPAGE_CONSOLE_OPTIONS)	// "advanced": the title of the page it belongs to
		page = optionsOrigin, title = TheText.Get(aScreens[optionsOrigin].m_ScreenName);
	if (page == MENUPAGE_CHOOSE_LOAD_SLOT || page == MENUPAGE_NEW_GAME || MenuSkinXboxDialog(page))
		title = TheText.Get("RVC_XTL");
	else if (page == MENUPAGE_CONTROLLER_PC)
		title = TheText.Get("RVC_XTC");
	else if (page == MENUPAGE_PAD_PICTURE)
		title = TheText.Get("RVC_XTD");
	alpha = DialogDim(alpha);
	CFont::SetRightJustifyOff();
	CFont::SetCentreOff();
	CFont::SetColor(CRGBA(0, 0, 0, alpha));
	CFont::PrintString(X(52.0f) + W(3.0f), Y(48.0f) + H(3.0f), title);
	CFont::SetColor(CRGBA(XBOX_PINK.r, XBOX_PINK.g, XBOX_PINK.b, alpha));
	CFont::PrintString(X(52.0f), Y(48.0f), title);
}

// The Vice City logo, top right (instead of the PC's top left), with the Xbox's glint: a white
// flash over "city" (r3_menu_230) then over "Vice" (232), 200 ms each from 3.4 s after the page
// started, repeating every 3.8 s (R3_Menu.anm anim 4; positions from the menu video).
// Test: draw the glint stage `stage` (1 or 2) whatever the time (REVC_TEST_GLINT).
static int TestGlint(void) { static int g = getenv("REVC_TEST_GLINT") ? atoi(getenv("REVC_TEST_GLINT")) : 0; return g; }
static int glintPage = -1;
static uint32 glintStart;

void
MenuSkinXboxLogo(uint8 alpha)
{
	alpha = DialogDim(alpha);
	const float x0 = X(458.0f), y0 = Y(36.0f), w = W(134.0f), h = H(67.0f);
	logo.Draw(CRect(x0, y0, x0 + w, y0 + h), CRGBA(255, 255, 255, alpha));
	uint32 now = CTimer::GetTimeInMillisecondsPauseMode();
	if (FrontEndMenuManager.m_nCurrScreen != glintPage) {
		glintPage = FrontEndMenuManager.m_nCurrScreen;
		glintStart = now;
	}
	uint32 t = (now - glintStart) % 3800;
	if (TestGlint())
		t = TestGlint() == 1 ? 3500 : 3700;
	if (t >= 3400 && t < 3600 && glint[0].m_pTexture)
		glint[0].Draw(CRect(x0 + 0.46f * w, y0 + 0.12f * h, x0 + 0.88f * w, y0 + 1.0f * h), CRGBA(255, 255, 255, alpha));
	else if (t >= 3600 && glint[1].m_pTexture)
		glint[1].Draw(CRect(x0 + 0.08f * w, y0 + 0.22f * h, x0 + 0.50f * w, y0 + 0.92f * h), CRGBA(255, 255, 255, alpha));
}


static void
DrawIconBar(CMenuManager *m, uint8 alpha)
{
	int cur = CurrentTab(m->m_nCurrScreen);
	for (int i = 0; i < 7; i++) {
		if (i == cur)
			continue;
		float s = ICON_SIZE;
		// the other pages' icons in grey, about 55 % (measured)
		icons[i].Draw(CRect(X(iconX[i]) - W(s / 2), Y(ICON_Y) - H(s / 2), X(iconX[i]) + W(s / 2), Y(ICON_Y) + H(s / 2)), CRGBA(140, 140, 140, alpha));
	}
	if (cur >= 0) {
		// the current page's icon, enlarged on the mint box (measured 482..580 x 368..425 for the TV)
		float cx = iconX[cur], s = ICON_SELECTED;
		if (barFocus)	// the green card only while the icon bar has focus
			CSprite2d::Draw2DPolygon(X(cx - 48.0f), Y(370.0f), X(cx + 50.0f), Y(367.0f), X(cx - 46.0f), Y(426.0f), X(cx + 48.0f), Y(424.0f),
				CRGBA(XBOX_MINT.r, XBOX_MINT.g, XBOX_MINT.b, alpha));
		icons[cur].Draw(CRect(X(cx) - W(s / 2), Y(ICON_Y + 2.0f) - H(s / 2), X(cx) + W(s / 2), Y(ICON_Y + 2.0f) + H(s / 2)), CRGBA(255, 255, 255, alpha));
	}
}

// "START: resume game   <-/->: select   A: select   B: back" in pink under the icons: the pad's
// glyphs (Xbox UI style: coloured) and the text in the Xbox's RWSFArborcrestHeavy (measured at y 427, 15 px).
// One help-line item: the pad's glyphs (up to two) or text, then ":" and the word.
static float
HelpItem(float x, float y, const int *glyphs, int ng, const char *pre, const char *word, uint8 alpha)
{
	const float gh = H(14.0f), th = H(15.0f);
	CRGBA pink(XBOX_PINK.r, XBOX_PINK.g, XBOX_PINK.b, alpha);
	for (int k = 0; k < ng; k++) {
		CSprite2d &g = CFont::ButtonSprite[glyphs[k]];
		if (g.m_pTexture) {
			g.Draw(CRect(x, y - H(1.0f), x + gh, y - H(1.0f) + gh), CRGBA(255, 255, 255, alpha));
			x += gh + W(1.0f);
		}
	}
	wchar buf[64];
	int n = 0;
	if (pre) {
		AsciiToUnicode(pre, buf);
		n = UnicodeStrlen(buf);
	}
	buf[n++] = ':';
	UnicodeStrcpy(buf + n, TheText.Get(word));
	if (fontRW.ok)
		XboxFontPrint(fontRW, x, y, th, buf, pink);
	return x + (fontRW.ok ? XboxFontWidth(fontRW, buf, th) : 0.0f);
}

// The help line under the icons, per state as on the Xbox (measured slots at x 52, 200, 340,
// 450): top level "START resume game, <-/-> select, A select"; in a list "START resume game, up/down
// select, A select, B back"; display controls "START resume game, <-/-> change, B back"; the map
// "A select, B back, thumbsticks move/zoom". The pad's glyphs, the text in RWSFArborcrestHeavy.
static void
DrawHelpLine(CMenuManager *m, uint8 alpha)
{
	const float y = Y(427.0f);
	static const int start[1] = { BUTTON_START }, lr[2] = { BUTTON_DPAD_LEFT, BUTTON_DPAD_RIGHT }, ud[2] = { BUTTON_DPAD_UP, BUTTON_DPAD_DOWN };
	static const int a[1] = { BUTTON_CROSS }, b[1] = { BUTTON_CIRCLE }, sticks[2] = { BUTTON_UP, BUTTON_RSTICK_UP };
	if (m->m_nCurrScreen == MENUPAGE_MAP) {
		HelpItem(X(52.0f), y, a, 1, nil, "RVC_XHS", alpha);
		HelpItem(X(200.0f), y, b, 1, nil, "RVC_XHB", alpha);
		HelpItem(X(340.0f), y, sticks, 2, nil, "RVC_XHM", alpha);
		return;
	}
	HelpItem(X(52.0f), y, start, 1, nil, "RVC_XHR", alpha);
	if (HasSubTabs(m->m_nCurrScreen) && (!SettingsSubTabs(m->m_nCurrScreen) || subBarFocus)) {	// "START:resume game  <-/->:select  B:back" (the sub-tabs)
		HelpItem(X(200.0f), y, lr, 2, nil, "RVC_XHS", alpha);
		HelpItem(X(340.0f), y, b, 1, nil, "RVC_XHB", alpha);
		return;
	}
	if (m->m_nCurrScreen == MENUPAGE_CONSOLE_OPTIONS) {	// "START:resume game  up/down:select  <-/->:change  B:back"
		HelpItem(X(200.0f), y, ud, 2, nil, "RVC_XHS", alpha);
		HelpItem(X(340.0f), y, lr, 2, nil, "RVC_XHC", alpha);
		HelpItem(X(450.0f), y, b, 1, nil, "RVC_XHB", alpha);
		return;
	}
	if (m->m_nCurrScreen == MENUPAGE_PAD_PICTURE) {
		HelpItem(X(200.0f), y, lr, 2, nil, "RVC_XHC", alpha);
		HelpItem(X(450.0f), y, b, 1, nil, "RVC_XHB", alpha);
		return;
	}
	bool bar = barFocus && !MenuSkinXboxDialog(m->m_nCurrScreen);	// a dialog is a list
	HelpItem(X(200.0f), y, bar ? lr : ud, 2, nil, "RVC_XHS", alpha);
	HelpItem(X(340.0f), y, a, 1, nil, "RVC_XHS", alpha);
	if (!bar)
		HelpItem(X(450.0f), y, b, 1, nil, "RVC_XHB", alpha);
	if (SettingsSubTabs(m->m_nCurrScreen)) {	// "Y:advanced" (the face-button item grammar of "X:delete")
		static const int yb[1] = { BUTTON_TRIANGLE };
		HelpItem(X(530.0f), y, yb, 1, nil, "RVC_XHA", alpha);
	}
}

// The mint selection bar behind the current item, with the Xbox's pulse: a white glow over it
// growing from 0.9x to 1.157 x 1.081 while fading from 0.5 to 0.059 alpha, every 0.8 s
// (R3_Menu.anm, decoded; docs/CONSOLE_STYLES.md 13h).
void
MenuSkinXboxSelection(float x0, float y0, float x1, float y1, uint8 alpha, bool tail, float minWidth)
{
	// Tilted with the panel (top edge leaning right), from the label's left edge to at least a
	// fixed width, the last part a darker mint tail (measured on the Xbox's list pages).
	x1 = Max(x1, x0 + W(minWidth));	// (dialog buttons: tight, minWidth 0)
	float tilt = W(5.0f), tailW = W(16.0f);
	if (!tail) {	// whole-row bars (the radio station) have no tail
		CSprite2d::Draw2DPolygon(x0 + tilt, y0, x1 + tilt, y0, x0, y1, x1, y1, CRGBA(XBOX_MINT.r, XBOX_MINT.g, XBOX_MINT.b, alpha));
	} else {
	CSprite2d::Draw2DPolygon(x0 + tilt, y0, x1 - tailW + tilt, y0, x0, y1, x1 - tailW, y1, CRGBA(XBOX_MINT.r, XBOX_MINT.g, XBOX_MINT.b, alpha));
	CSprite2d::Draw2DPolygon(x1 - tailW + tilt, y0, x1 + tilt, y0, x1 - tailW, y1, x1, y1, CRGBA(0, 120, 80, alpha));
	}
	// the pulse: a white glow over the bar growing 0.9 -> 1.157 x 1.081 while fading 0.5 -> 0.059,
	// every 0.8 s (R3_Menu.anm)
	float t = (CTimer::GetTimeInMillisecondsPauseMode() % 800) / 800.0f;
	float sx = 0.9f + (1.157f - 0.9f) * t, sy = 0.9f + (1.081f - 0.9f) * t;
	float a = 0.5f + (0.059f - 0.5f) * t;
	float cx = (x0 + x1) / 2.0f, cy = (y0 + y1) / 2.0f, hw = (x1 - x0) / 2.0f * sx, hh = (y1 - y0) / 2.0f * sy;
	// the glow grows only vertically past the bar: wider, it shows as a grey box where the bar
	// meets black (the radio row, the map legend; rounds 7-8)
	hw = Min(hw, (x1 - x0) / 2.0f);
	CSprite2d::DrawRect(CRect(cx - hw, cy - hh, cx + hw, cy + hh), CRGBA(255, 255, 255, (uint8)(a * alpha)));
}

// A dialog's background, drawn before the page's own text (CMenuManager::DrawStandardMenus): the
// menu behind dimmed, then its layers as measured on the new-game dialog: a steel-blue layer
// behind, offset down-right, a dark edge, and the light-blue panel.
void
MenuSkinXboxDialogBack(uint8 alpha)
{
	CSprite2d::DrawRect(CRect(0.0f, 0.0f, SCREEN_WIDTH, SCREEN_HEIGHT), CRGBA(0, 0, 0, alpha / 2));
	CSprite2d::Draw2DPolygon(X(118.0f), Y(152.0f), X(560.0f), Y(136.0f), X(118.0f), Y(352.0f), X(538.0f), Y(347.0f), CRGBA(70, 118, 145, alpha));
	CSprite2d::Draw2DPolygon(X(522.0f), Y(128.0f), X(543.0f), Y(126.0f), X(510.0f), Y(350.0f), X(531.0f), Y(350.0f), CRGBA(90, 160, 205, alpha));	// stacked lighter edges on the right
	CSprite2d::Draw2DPolygon(X(498.0f), Y(126.0f), X(520.0f), Y(124.0f), X(484.0f), Y(352.0f), X(506.0f), Y(352.0f), CRGBA(20, 40, 60, alpha));
	CSprite2d::Draw2DPolygon(X(85.0f), Y(138.0f), X(520.0f), Y(118.0f), X(100.0f), Y(337.0f), X(505.0f), Y(333.0f), CRGBA(105, 198, 247, alpha));
}

// The Xbox's slider: a rising wedge from x 325 to 485 (12 px tall at the left, 33 at the right,
// bottom 21 px under the row's top), faint where empty, with bright bars up to the value
// (measured on the Display page). Returns the right end of the filled part (the PC slider's
// "last active bar", used for mouse hover).
float
MenuSkinXboxWedge(float y, float progress, uint8 alpha)
{
	const float x0 = X(325.0f), x1 = X(485.0f), bottom = y + H(21.0f);
	auto top = [&](float x) { return bottom - H(12.0f) - (x - x0) / (x1 - x0) * H(21.0f); };
	CSprite2d::Draw2DPolygon(x0, top(x0), x1, top(x1), x0, bottom, x1, bottom, CRGBA(120, 170, 220, alpha * 90 / 255));
	float filled = x0 + (x1 - x0) * Clamp(progress, 0.0f, 1.0f);
	for (float x = x0; x + W(3.0f) <= filled; x += W(6.0f))
		CSprite2d::DrawRect(CRect(x, top(x + W(1.5f)), x + W(3.0f), bottom), CRGBA(110, 190, 245, alpha));
	return filled;
}

// Sub-tabs on the Stats ("stats | unlocked items") and Brief ("briefs | tutorial briefings")
// pages, at the bottom left: the active one on a mint bar with a dark block at its outer end
// (measured at 640x480: labels at x 68 and 248 / 308, y 350; bars 60-240 with the block at
// 45-60, and 300-590 with the block at 590-605). <-/-> switches.
static int subTab = 0, subTabPage = -1;

static bool
SettingsSubTabs(int page)
{
	return page == MENUPAGE_CONSOLE_DISPLAY || page == MENUPAGE_SOUND_SETTINGS || page == MENUPAGE_CONSOLE_CONTROLS || page == MENUPAGE_CONSOLE_OPTIONS;
}

static bool
HasSubTabs(int page)
{
	// Display, Audio and Controls stay as on the console (Y opens their "advanced" list); the
	// list shows "<page> | advanced" where Stats has its sub-tabs
	return page == MENUPAGE_STATS || page == MENUPAGE_BRIEFS || page == MENUPAGE_CONSOLE_OPTIONS;
}

int
MenuSkinXboxSubTab(int page)
{
	if (SettingsSubTabs(page))
		return page == MENUPAGE_CONSOLE_OPTIONS ? 1 : 0;
	return page == subTabPage ? subTab : 0;
}

// Test: the "advanced" list of a section (0 display, 1 audio, 2 controls) as if opened from its page.
extern "C" void
MenuSkinXboxOptionsFor(int section)
{
	extern int gConsoleOptionsSection;
	optionsOrigin = section == 1 ? MENUPAGE_SOUND_SETTINGS : section == 2 ? MENUPAGE_CONSOLE_CONTROLS : MENUPAGE_CONSOLE_DISPLAY;
	gConsoleOptionsSection = section;
	ConsoleAudioRows(true);
}

extern "C" void	// (C linkage: called from skel/host/testhooks.cpp)
MenuSkinXboxSetSubTab(int page, int tab)
{
	subTabPage = page;
	subTab = tab;
}

static void
DrawSubTabs(int page, uint8 alpha)
{
	const bool stats = page == MENUPAGE_STATS;
	const char *names[2] = { stats ? "RVC_XS1" : "RVC_XB1", stats ? "RVC_XS2" : "RVC_XB2" };
	float lx[2] = { 68.0f, stats ? 248.0f : 308.0f };
	if (SettingsSubTabs(page)) {	// "display | advanced" and so on (OPTIONS_PLACEMENT)
		int origin = page == MENUPAGE_CONSOLE_OPTIONS ? optionsOrigin : page;
		names[0] = origin == MENUPAGE_SOUND_SETTINGS ? "RVC_XAU" : origin == MENUPAGE_CONSOLE_CONTROLS ? "RVC_XCO" : "RVC_XDI";
		names[1] = "RVC_XAD";
		lx[1] = 248.0f;
	}
	const float ty = 350.0f * 448.0f / 480.0f, y0 = Y(340.0f * 448.0f / 480.0f), y1 = Y(375.0f * 448.0f / 480.0f);
	int cur = MenuSkinXboxSubTab(page);
	CFont::SetFontStyle(FONT_LOCALE(FONT_HEADING));
	CFont::SetScale(W(BIGTEXT_X_SCALE * 1.2f), H(BIGTEXT_Y_SCALE * 1.2f));
	CFont::SetPropOn();
	CFont::SetCentreOff();
	CFont::SetRightJustifyOff();
	CFont::SetWrapx(SCREEN_WIDTH);
	float w = CFont::GetStringWidth(TheText.Get(names[cur]), true);
	float bx0 = X(lx[cur] - 8.0f), bx1 = Max(X(lx[cur]) + w + W(60.0f), X(lx[cur] + 172.0f)), tilt = W(5.0f);
	CSprite2d::Draw2DPolygon(bx0 + tilt, y0, bx1 + tilt, y0, bx0, y1, bx1, y1, CRGBA(XBOX_MINT.r, XBOX_MINT.g, XBOX_MINT.b, alpha));
	if (cur == 0)	// the dark block at the bar's outer end
		CSprite2d::Draw2DPolygon(bx0 - W(15.0f) + tilt, y0, bx0 + tilt, y0, bx0 - W(15.0f), y1, bx0, y1, CRGBA(40, 40, 40, alpha));
	else
		CSprite2d::Draw2DPolygon(bx1 + tilt, y0, bx1 + W(15.0f) + tilt, y0, bx1, y1, bx1 + W(15.0f), y1, CRGBA(40, 40, 40, alpha));
	CFont::SetDropShadowPosition(2);
	CFont::SetDropColor(CRGBA(0, 0, 0, alpha));
	CFont::SetColor(CRGBA(XBOX_PINK.r, XBOX_PINK.g, XBOX_PINK.b, alpha));
	for (int i = 0; i < 2; i++)
		CFont::PrintString(X(lx[i]), Y(ty), TheText.Get(names[i]));
	CFont::SetDropShadowPosition(0);
}

// LT/RT step through the pages, as on the Xbox pause menu. True when it switched.
bool
MenuSkinXboxInput(CMenuManager *m)
{
	if (!MenuSkinXboxActive(m->m_nCurrScreen, m->m_bGameNotLoaded))
		return false;
	CPad *pad = CPad::GetPad(0);
	const bool left = pad->GetDPadLeftJustDown() || pad->GetLeftJustDown(), right = pad->GetDPadRightJustDown() || pad->GetRightJustDown();
	const bool up = pad->GetDPadUpJustDown() || pad->GetUpJustDown(), down = pad->GetDPadDownJustDown() || pad->GetDownJustDown();
	(void)left; (void)right; (void)up; (void)down;
	subBarFocus = false;
	// Y (the help line's "Y:advanced") on Display, Audio and Controls opens their "advanced"
	// list; B there goes back to the page
	if (SettingsSubTabs(m->m_nCurrScreen) && m->m_nCurrScreen != MENUPAGE_CONSOLE_OPTIONS && pad->GetTriangleJustDown()) {
		extern int gConsoleOptionsSection;
		optionsOrigin = m->m_nCurrScreen;
		gConsoleOptionsSection = optionsOrigin == MENUPAGE_SOUND_SETTINGS ? 1 : optionsOrigin == MENUPAGE_CONSOLE_CONTROLS ? 2 : 0;
		ConsoleAudioRows(true);
		aScreens[MENUPAGE_CONSOLE_OPTIONS].m_PreviousPage = optionsOrigin;
		m->SwitchToNewScreen(MENUPAGE_CONSOLE_OPTIONS);
		return true;
	}
	if (HasSubTabs(m->m_nCurrScreen) && !SettingsSubTabs(m->m_nCurrScreen) && (pad->GetDPadLeftJustDown() || pad->GetDPadRightJustDown() || pad->GetLeftJustDown() || pad->GetRightJustDown())) {
		MenuSkinXboxSetSubTab(m->m_nCurrScreen, 1 - MenuSkinXboxSubTab(m->m_nCurrScreen));
		barFocus = false;
		return true;
	}
	int step = pad->GetRightShoulder2JustDown() ? 1 : pad->GetLeftShoulder2JustDown() ? -1 : 0;
	if (step == 0)
		return false;
	int n = ARRAY_SIZE(tabs);
	m->SwitchToNewScreen(tabs[(CurrentTab(m->m_nCurrScreen) + step + n) % n].page);
	barFocus = true;
	barFocusOption = -1;
	return true;
}

// Called by CMenuManager::DrawBackground after the page.
void
MenuSkinXboxDraw(CMenuManager *m, uint8 alpha)
{
	if (!MenuSkinXboxActive(m->m_nCurrScreen, m->m_bGameNotLoaded))
		return;
	// focus leaves the icon bar once the player moves in the page's list
	if (barFocus) {
		if (barFocusOption < 0)
			barFocusOption = m->m_nCurrOption;
		else if (m->m_nCurrOption != barFocusOption)
			barFocus = false;
	}
	if (HasSubTabs(m->m_nCurrScreen))
		DrawSubTabs(m->m_nCurrScreen, alpha);
	DrawIconBar(m, DialogDim(alpha));
	DrawHelpLine(m, DialogDim(alpha));
	CFont::DrawFonts();
}
