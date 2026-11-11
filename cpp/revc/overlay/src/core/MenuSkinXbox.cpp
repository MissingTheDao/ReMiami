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

const CRGBA XBOX_MINT(0, 210, 141, 255);
const CRGBA XBOX_PINK(255, 154, 227, 255);

// The Xbox pause menu's seven pages in icon order, the PC page shown for each, the icon sprite.
struct XboxTab { int page; int alt; const char *icon; };
static const XboxTab tabs[] = {
	{ MENUPAGE_CHOOSE_LOAD_SLOT, -1, "ui/r3_menu_238.dds" },	// disk: load
	{ MENUPAGE_MAP, -1, "ui/r3_menu_241.dds" },			// compass
	{ MENUPAGE_BRIEFS, -1, "ui/r3_menu_243.dds" },		// tape
	{ MENUPAGE_STATS, -1, "ui/r3_menu_246.dds" },			// computer
	{ MENUPAGE_CONTROLLER_PC, MENUPAGE_PAD_PICTURE, "ui/r3_menu_249.dds" },	// pad (and the controller picture)
	{ MENUPAGE_SOUND_SETTINGS, -1, "ui/r3_menu_252.dds" },	// boombox
	{ MENUPAGE_DISPLAY_SETTINGS, MENUPAGE_GRAPHICS_SETTINGS, "ui/r3_menu_255.dds" },	// TV
};
// icon centres on the bar (measured)
static const float iconX[7] = { 112.0f, 180.0f, 253.0f, 325.0f, 395.0f, 463.0f, 530.0f };
static const float ICON_Y = 395.0f, ICON_SIZE = 50.0f, ICON_SELECTED = 80.0f;

// Heap-allocated so nothing runs at program exit (after RenderWare is gone); freed with the menu's
// own textures by MenuSkinXboxUnload.
static CSprite2d *icons, *logoSprite, *glint;
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

static bool
LoadSprites(void)
{
	if (spritesState == 0) {
		spritesState = -1;
		if (icons == nil) {
			icons = new CSprite2d[7];
			logoSprite = new CSprite2d;
			glint = new CSprite2d[2];
		}
		bool ok = true;
		for (int i = 0; i < 7; i++)
			ok = ok && (icons[i].m_pTexture = ImportTexture(tabs[i].icon)) != nil;
		logo.m_pTexture = ImportTexture("ui/r3_menu_228.dds");
		glint[0].m_pTexture = ImportTexture("ui/r3_menu_230.dds");
		glint[1].m_pTexture = ImportTexture("ui/r3_menu_232.dds");
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
	if (fontRW.sprite)
		fontRW.sprite->Delete();
	fontRW.ok = false;
	spritesState = 0;
}

// The confirmation dialogs reached from the Load page: drawn as the Xbox's layered blue dialog
// over the dimmed Load page.
bool
MenuSkinXboxDialog(int page)
{
	return page == MENUPAGE_NEW_GAME_RELOAD || page == MENUPAGE_LOAD_SLOT_CONFIRM || page == MENUPAGE_DELETE_SLOT_CONFIRM;
}

static int
CurrentTab(int page)
{
	if (MenuSkinXboxDialog(page))
		return 0;
	for (int i = 0; i < (int)ARRAY_SIZE(tabs); i++)
		if (tabs[i].page == page || tabs[i].alt == page)
			return i;
	return -1;
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
		icons[i].Draw(CRect(X(iconX[i]) - W(s / 2), Y(ICON_Y) - H(s / 2), X(iconX[i]) + W(s / 2), Y(ICON_Y) + H(s / 2)), CRGBA(255, 255, 255, alpha));
	}
	if (cur >= 0) {
		// the current page's icon, enlarged on the mint box (measured 482..580 x 368..425 for the TV)
		float cx = iconX[cur], s = ICON_SELECTED;
		CSprite2d::Draw2DPolygon(X(cx - 48.0f), Y(370.0f), X(cx + 50.0f), Y(367.0f), X(cx - 46.0f), Y(426.0f), X(cx + 48.0f), Y(424.0f),
			CRGBA(XBOX_MINT.r, XBOX_MINT.g, XBOX_MINT.b, alpha));
		icons[cur].Draw(CRect(X(cx) - W(s / 2), Y(ICON_Y + 2.0f) - H(s / 2), X(cx) + W(s / 2), Y(ICON_Y + 2.0f) + H(s / 2)), CRGBA(255, 255, 255, alpha));
	}
}

// "START: resume game   <-/->: select   A: select   B: back" in pink under the icons: the pad's
// glyphs (Xbox UI style: coloured) and the text in the Xbox's RWSFArborcrestHeavy (measured at y 427, 15 px).
static void
DrawHelpLine(uint8 alpha)
{
	static const int glyphs[4][2] = { { BUTTON_START, -1 }, { BUTTON_DPAD_LEFT, BUTTON_DPAD_RIGHT }, { BUTTON_CROSS, -1 }, { BUTTON_CIRCLE, -1 } };
	static const char *texts[4] = { "RVC_XHR", "RVC_XHS", "RVC_XHS", "RVC_XHB" };
	static const float px[4] = { 52.0f, 200.0f, 340.0f, 450.0f };
	const float gh = H(14.0f), th = H(15.0f), y = Y(427.0f);
	CRGBA pink(XBOX_PINK.r, XBOX_PINK.g, XBOX_PINK.b, alpha);
	wchar buf[64];
	for (int i = 0; i < 4; i++) {
		float x = X(px[i]);
		for (int k = 0; k < 2; k++) {
			if (glyphs[i][k] < 0)
				continue;
			CSprite2d &g = CFont::ButtonSprite[glyphs[i][k]];
			if (g.m_pTexture) {
				g.Draw(CRect(x, y - H(1.0f), x + gh, y - H(1.0f) + gh), CRGBA(255, 255, 255, alpha));
				x += gh + W(1.0f);
			}
		}
		buf[0] = ':';
		UnicodeStrcpy(buf + 1, TheText.Get(texts[i]));
		if (fontRW.ok)
			XboxFontPrint(fontRW, x, y, th, buf, pink);
		else {
			CFont::SetFontStyle(FONT_LOCALE(FONT_STANDARD));
			CFont::SetScale(W(0.44f), H(0.74f));
			CFont::SetColor(pink);
			CFont::PrintString(x, y, buf);
		}
	}
}

// The mint selection bar behind the current item, with the Xbox's pulse: a white glow over it
// growing from 0.9x to 1.157 x 1.081 while fading from 0.5 to 0.059 alpha, every 0.8 s
// (R3_Menu.anm, decoded; docs/CONSOLE_STYLES.md 13h).
void
MenuSkinXboxSelection(float x0, float y0, float x1, float y1, uint8 alpha)
{
	CSprite2d::DrawRect(CRect(x0, y0, x1, y1), CRGBA(XBOX_MINT.r, XBOX_MINT.g, XBOX_MINT.b, alpha));
	float t = (CTimer::GetTimeInMillisecondsPauseMode() % 800) / 800.0f;
	float sx = 0.9f + (1.157f - 0.9f) * t, sy = 0.9f + (1.081f - 0.9f) * t;
	float a = 0.5f + (0.059f - 0.5f) * t;
	float cx = (x0 + x1) / 2.0f, cy = (y0 + y1) / 2.0f, hw = (x1 - x0) / 2.0f * sx, hh = (y1 - y0) / 2.0f * sy;
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
	CSprite2d::Draw2DPolygon(X(498.0f), Y(126.0f), X(520.0f), Y(124.0f), X(484.0f), Y(352.0f), X(506.0f), Y(352.0f), CRGBA(20, 40, 60, alpha));
	CSprite2d::Draw2DPolygon(X(85.0f), Y(138.0f), X(520.0f), Y(118.0f), X(100.0f), Y(337.0f), X(505.0f), Y(333.0f), CRGBA(105, 198, 247, alpha));
}

// LT/RT step through the pages, as on the Xbox pause menu. True when it switched.
bool
MenuSkinXboxInput(CMenuManager *m)
{
	if (!MenuSkinXboxActive(m->m_nCurrScreen, m->m_bGameNotLoaded))
		return false;
	CPad *pad = CPad::GetPad(0);
	int step = pad->GetRightShoulder2JustDown() ? 1 : pad->GetLeftShoulder2JustDown() ? -1 : 0;
	if (step == 0)
		return false;
	int n = ARRAY_SIZE(tabs);
	m->SwitchToNewScreen(tabs[(CurrentTab(m->m_nCurrScreen) + step + n) % n].page);
	return true;
}

// Called by CMenuManager::DrawBackground after the page.
void
MenuSkinXboxDraw(CMenuManager *m, uint8 alpha)
{
	if (!MenuSkinXboxActive(m->m_nCurrScreen, m->m_bGameNotLoaded))
		return;
	DrawIconBar(m, alpha);
	DrawHelpLine(alpha);
	CFont::DrawFonts();
}
