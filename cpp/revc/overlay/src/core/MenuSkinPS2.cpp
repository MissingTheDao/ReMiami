// revc (plan task 13g): PS2 skin for the PC menu, with UI STYLE = PS2.
//
// The menu logic, pages and options stay the PC menu's; this draws what the PS2 pause menu has
// on top of the shared panel, logo and title: the page tabs at the bottom (two rows, the current
// one on a slanted green box) and the green help box with the buttons. Positions and colours are
// measured on the PS2 captures (PCSX2, Display page, 1440x1080 = the 640x448 frame scaled to 4:3)
// and given here in those capture pixels; docs/CONSOLE_STYLES.md.
// re3's Frontend_PS2.cpp is GTA III's PS2 menu and is not used.
#include "common.h"
#include "Frontend.h"
#include "Font.h"
#include "Sprite2d.h"
#include "Text.h"
#include "Timer.h"
#include "Pad.h"

extern int8 gUiStyle;
extern bool gGlyphPackLoaded;

// Capture pixels (1440x1080) to the menu's 640x448 layout space, scaled and centred like the
// PC menu (SCALE_AND_CENTER_X / SCREEN_SCALE_Y; the PS2 UI style lays out in 448 lines).
static float PX(float x) { return SCALE_AND_CENTER_X(x * 640.0f / 1440.0f); }
static float PY(float y) { return SCREEN_SCALE_Y(y * 448.0f / 1080.0f); }
static float SX(float s) { return SCREEN_SCALE_X(s); }

// The help box sits on the background panel's bottom-right corner on every PS2 page (on purpose:
// it covers the corner and overhangs it). The panel changes shape per page and stretches to wide
// screens, so the box is placed relative to that corner: capture pixels relative to the PS2
// Display page's corner (1345, 870), scaled like the rest.
static float gCornerX, gCornerY;
static float HX(float x) { return gCornerX + SCREEN_SCALE_X((x - 1345.0f) * 640.0f / 1440.0f); }
static float HY(float y) { return gCornerY + SCREEN_SCALE_Y((y - 870.0f) * 448.0f / 1080.0f); }
static float SY(float s) { return SCREEN_SCALE_Y(s); }

static const CRGBA PS2_GREEN(25, 130, 70, 255);
static const CRGBA PS2_TAB_TEXT(211, 211, 211, 255);

// The PS2 pause menu's eight pages, in tab order (two rows of four), and the PC page shown for
// each; centres of the tab labels in capture pixels.
struct Ps2Tab { int page; int alt; float x, y; const char *name; };
static const Ps2Tab tabs[] = {
	{ MENUPAGE_MAP, -1, 400.0f, 942.0f, "FEH_MAP" },
	{ MENUPAGE_BRIEFS, -1, 577.0f, 942.0f, "FEH_BRI" },
	{ MENUPAGE_CHOOSE_LOAD_SLOT, -1, 755.0f, 942.0f, "RVC_PT2" },
	{ MENUPAGE_STATS, -1, 946.0f, 942.0f, "FEP_STA" },
	{ MENUPAGE_CONSOLE_CONTROLS, MENUPAGE_PAD_PICTURE, 332.0f, 995.0f, "RVC_PT4" },	// the console's controls page
	{ MENUPAGE_SOUND_SETTINGS, -1, 568.0f, 995.0f, "RVC_PT5" },
	{ MENUPAGE_CONSOLE_DISPLAY, -1, 785.0f, 995.0f, "RVC_PT6" },	// the console's display page
	{ MENUPAGE_CONSOLE_OPTIONS, MENUPAGE_LANGUAGE_SETTINGS, 1043.0f, 995.0f, "RVC_PT8" },	// revc: "options" in the EU discs' "language" place
};

// The PS2's highlight boxes glide: to a new item in 167 ms (the label's size morphing on the way),
// and in from 190 px below when a page opens (measured on the v3.00 video). id: one box kind
// (0 the tab bar, 1 the slot lists); returns the box to draw this frame.
CRect
MenuSkinPS2Glide(int id, int page, const CRect &target)
{
	static struct { CRect from, to, cur; uint32 start; int page; bool init; } g[4];
	auto &s = g[id];
	uint32 now = CTimer::GetTimeInMillisecondsPauseMode();
	if (!s.init || s.page != page) {
		s.init = true;
		s.page = page;
		s.from = target;
		s.from.top += SY(190.0f);
		s.from.bottom += SY(190.0f);
		s.to = target;
		s.start = now;
	} else if (target.left != s.to.left || target.top != s.to.top || target.right != s.to.right || target.bottom != s.to.bottom) {
		s.from = s.cur;
		s.to = target;
		s.start = now;
	}
	float t = Min(1.0f, (now - s.start) / (s.from.top - s.to.top > SY(100.0f) ? 100.0f : 167.0f));	// (the page-open sweep: 3 frames)
	s.cur = CRect(s.from.left + (s.to.left - s.from.left) * t, s.from.top + (s.to.top - s.from.top) * t,
		s.from.right + (s.to.right - s.from.right) * t, s.from.bottom + (s.to.bottom - s.from.bottom) * t);
	return s.cur;
}

// "screen position" (PS2 Display page): the picture's offset for TVs that crop it, as on the PS2.
// Packed in one ini value ([Display] ScreenPosition): x and y, -8..7 steps of 2 menu pixels each.
// X on the row starts adjusting with the arrows; X or triangle ends it.
int8 gScreenPosition = (int8)0x88;	// x 0, y 0
bool gScreenPositionAdjust = false;
float ScreenPositionX(void) { return ((((uint8)gScreenPosition) >> 4) - 8) * 2.0f; }
float ScreenPositionY(void) { return ((((uint8)gScreenPosition) & 15) - 8) * 2.0f; }

// Focus on the Map page: on the tab bar (the "map" tab boxed) until up/down moves it to the
// legend item (then only the legend is boxed), as on the PS2.
static int focusPage = -1;
static bool pageFocus = false;

static int
CurrentTab(int page)
{
	// the load/new-game confirmations belong to the load tab, as on the PS2
	if (page == MENUPAGE_NEW_GAME_RELOAD || page == MENUPAGE_LOAD_SLOT_CONFIRM || page == MENUPAGE_DELETE_SLOT_CONFIRM)
		page = MENUPAGE_CHOOSE_LOAD_SLOT;
	for (int i = 0; i < (int)ARRAY_SIZE(tabs); i++)
		if (tabs[i].page == page || tabs[i].alt == page)
			return i;
	return -1;
}

// The tab bar: two rows under the panel, the current tab on a green box just around its label,
// slightly tilted. revc keeps the EU's eight tabs (language too), so the EU v3.00 layout (US v1.40
// has seven, x about 158-453, y 404 / 426).
static const float tabCX[8] = { 178, 256, 336, 420, 148, 252, 349, 464 };	// EU v3.00 (8 tabs)
static const float tabRowY[2] = { 392, 414 };

static void
DrawTabs(CMenuManager *m, uint8 alpha)
{
	int cur = CurrentTab(m->m_nCurrScreen);
	if (cur < 0)
		return;
	CFont::SetFontStyle(FONT_LOCALE(FONT_HEADING));
	CFont::SetScale(SX(0.68f), SY(0.92f));
	CFont::SetCentreOn();
	CFont::SetCentreSize(SCREEN_WIDTH);
	CFont::SetPropOn();
	CFont::SetBackgroundOff();
	CFont::SetWrapx(SCREEN_WIDTH);
	// the confirmations show the tabs without a highlight (console)
	bool confirm = m->m_nCurrScreen == MENUPAGE_NEW_GAME_RELOAD || m->m_nCurrScreen == MENUPAGE_LOAD_SLOT_CONFIRM ||
		m->m_nCurrScreen == MENUPAGE_DELETE_SLOT_CONFIRM;
	if (!confirm && !(m->m_nCurrScreen == MENUPAGE_MAP && pageFocus)) {
		float w = CFont::GetStringWidth(TheText.Get(tabs[cur].name), true);
		float cx = SCALE_AND_CENTER_X(tabCX[cur]), cy = SCREEN_SCALE_Y(tabRowY[cur / 4]);
		float hw = w / 2.0f + SX(8.0f), h0 = SY(13.0f), t = SY(2.0f);
		// gliding from tab to tab (one box for the bar: no sweep on page changes)
		CRect r = MenuSkinPS2Glide(0, 0, CRect(cx - hw, cy - h0, cx + hw, cy + h0));
		cx = (r.left + r.right) / 2.0f;
		hw = (r.right - r.left) / 2.0f;
		cy = (r.top + r.bottom) / 2.0f;
		CSprite2d::Draw2DPolygon(cx - hw, cy - h0 + t, cx + hw, cy - h0 - t, cx - hw, cy + h0 + t, cx + hw, cy + h0 - t,
			CRGBA(PS2_GREEN.r, PS2_GREEN.g, PS2_GREEN.b, alpha));
	}
	CFont::SetDropShadowPosition(2);
	CFont::SetDropColor(CRGBA(0, 0, 0, alpha));
	CFont::SetColor(CRGBA(PS2_TAB_TEXT.r, PS2_TAB_TEXT.g, PS2_TAB_TEXT.b, alpha));
	for (int i = 0; i < (int)ARRAY_SIZE(tabs); i++)
		CFont::PrintString(SCALE_AND_CENTER_X(tabCX[i]), SCREEN_SCALE_Y(tabRowY[i / 4] - 9.0f), TheText.Get(tabs[i].name));
	CFont::SetCentreOff();
	CFont::SetDropShadowPosition(0);
}

// page: the Map page has a fifth row (R1 \ R2 button - Zoom) and "Scroll" for the arrows; the
// Save page navigates with up/down only (as on the PS2).
static void
DrawHelpBox(uint8 alpha, int page)
{
	const bool mapPage = page == MENUPAGE_MAP && pageFocus, savePage = page == MENUPAGE_CHOOSE_SAVE_SLOT, optionsPage = page == MENUPAGE_CONSOLE_OPTIONS;
	const int rows = mapPage ? 5 : 4, nd = savePage || optionsPage ? 2 : 4;	// (the options list: up/down scroll)
	// thick black outline, about 8 capture pixels all round (measured)
	const float o = 8.0f;
	CSprite2d::Draw2DPolygon(HX(975.0f - o), HY(742.0f - o), HX(1390.0f + o), HY(728.0f - o), HX(990.0f - o), HY(882.0f + o), HX(1418.0f + o), HY(910.0f + o),
		CRGBA(0, 0, 0, alpha));
	CSprite2d::Draw2DPolygon(HX(975.0f), HY(742.0f), HX(1390.0f), HY(728.0f), HX(990.0f), HY(882.0f), HX(1418.0f), HY(910.0f),
		CRGBA(PS2_GREEN.r, PS2_GREEN.g, PS2_GREEN.b, alpha));
	// Select/Back: the pad's buttons as outline-only glyphs in black, like the text (the PS2 UI
	// style loads the glyph pack's outline variant, white line art, tinted here). Without the
	// pack the game's own coloured button textures are drawn as they are.
	static const int glyphs[3] = { BUTTON_CROSS, BUTTON_TRIANGLE, BUTTON_START };
	static const int dpad[4] = { BUTTON_DPAD_UP, BUTTON_DPAD_DOWN, BUTTON_DPAD_RIGHT, BUTTON_DPAD_LEFT };
	const char *actions[5] = { optionsPage ? "RVC_HCH" : "RVC_HSE", "RVC_HBA", "RVC_HRE", mapPage || optionsPage ? "RVC_HSC" : "RVC_HNA", "RVC_HZO" };
	CFont::SetFontStyle(FONT_LOCALE(FONT_STANDARD));
	CFont::SetScale(SX(mapPage ? 0.36f : 0.41f), SY(mapPage ? 0.62f : 0.68f));	// five rows: a little smaller to fit the box
	CFont::SetPropOn();
	CFont::SetBackgroundOff();
	CFont::SetDropShadowPosition(0);
	CFont::SetColor(CRGBA(0, 0, 0, alpha));	// black text on the green, as on the PS2
	const CRGBA glyphColor = gGlyphPackLoaded ? CRGBA(0, 0, 0, alpha) : CRGBA(255, 255, 255, alpha);
	const float sy = CFont::Details.scaleY;
	const float gs = 21.0f * sy;	// a little larger than the font's glyph cell, like the PS2's symbols
	// The rows are centred in the box as a block: "<glyph> button" right-aligned before a common
	// dash column, the actions after it (as on the PS2). Widths first, then the column position.
	const float gap = sy * 4.0f;
	const float dashGap = SCREEN_SCALE_X(10.0f * 640.0f / 1440.0f), actionGap = SCREEN_SCALE_X(40.0f * 640.0f / 1440.0f);
	const bool dpadGlyphs = gGlyphPackLoaded && CFont::ButtonSprite[dpad[0]].m_pTexture;
	wchar left[3][64];
	float leftW[5], rightW = 0.0f, leftMax = 0.0f;
	for (int i = 0; i < 3; i++) {
		CSprite2d &g = CFont::ButtonSprite[glyphs[i]];
		if (g.m_pTexture == nil && i == 2) {
			// without a Start glyph: "START button"
			UnicodeStrcpy(left[i], TheText.Get("RVC_HST"));
			int n = UnicodeStrlen(left[i]);
			left[i][n++] = ' ';
			UnicodeStrcpy(left[i] + n, TheText.Get("RVC_HBT"));
		} else
			UnicodeStrcpy(left[i], TheText.Get("RVC_HBT"));
		leftW[i] = CFont::GetStringWidth(left[i], true) + (g.m_pTexture ? gap + gs : 0.0f);
	}
	leftW[3] = dpadGlyphs ? nd * gs + (nd - 1) * sy * 2.0f : (HX(1202.0f) - HX(1068.0f)) * (savePage ? 0.5f : 1.0f);
	// the zoom row: R1 \ R2 glyphs then "button" (text "R1 \ R2 button" without the glyphs)
	wchar zoomSep[4], zoomText[48];
	AsciiToUnicode(" \\ ", zoomSep);
	const bool shoulderGlyphs = gGlyphPackLoaded && CFont::ButtonSprite[BUTTON_R1].m_pTexture && CFont::ButtonSprite[BUTTON_R2].m_pTexture;
	if (shoulderGlyphs)
		UnicodeStrcpy(zoomText, TheText.Get("RVC_HBT"));
	else
		UnicodeStrcpy(zoomText, TheText.Get("RVC_HZB"));
	leftW[4] = shoulderGlyphs ? 2.0f * gs + CFont::GetStringWidth(zoomSep, true) + gap + CFont::GetStringWidth(zoomText, true) : CFont::GetStringWidth(zoomText, true);
	for (int i = 0; i < rows; i++) {
		leftMax = Max(leftMax, leftW[i]);
		rightW = Max(rightW, CFont::GetStringWidth(TheText.Get(actions[i]), true));
	}
	// the box's centre (capture pixels, measured) and the dash column that centres the block
	const float centre = HX(1196.0f);
	const float column = centre - (leftMax + actionGap + rightW) / 2.0f + leftMax;

	// vertically too: the four rows (36 capture pixels apart, glyph-high) centred on the box's
	// centre, the average of its corners (y 815.5)
	const float step = SCREEN_SCALE_Y((mapPage ? 29.0f : 36.0f) * 448.0f / 1080.0f);	// five rows sit closer
	const float top = HY(815.5f) - ((rows - 1) * step + gs) / 2.0f;

	for (int i = 0; i < rows; i++) {
		float y = top + step * i;
		if (i < 3) {
			CSprite2d &g = CFont::ButtonSprite[glyphs[i]];
			float w = CFont::GetStringWidth(left[i], true);
			float gx = column - w - gap - gs;
			if (g.m_pTexture)
				g.Draw(CRect(gx, y + 0.5f * sy, gx + gs, y + 0.5f * sy + gs), glyphColor);
			CFont::SetRightJustifyOn();
			CFont::PrintString(column, y, left[i]);
			CFont::SetRightJustifyOff();
		} else if (i == 4) {
			if (shoulderGlyphs) {
				float gx = column - leftW[4];
				CFont::ButtonSprite[BUTTON_R1].Draw(CRect(gx, y + 0.5f * sy, gx + gs, y + 0.5f * sy + gs), glyphColor);
				gx += gs;
				CFont::PrintString(gx, y, zoomSep);
				gx += CFont::GetStringWidth(zoomSep, true);
				CFont::ButtonSprite[BUTTON_R2].Draw(CRect(gx, y + 0.5f * sy, gx + gs, y + 0.5f * sy + gs), glyphColor);
			}
			CFont::SetRightJustifyOn();
			CFont::PrintString(column, y, zoomText);
			CFont::SetRightJustifyOff();
		} else if (dpadGlyphs) {
			// Navigate: the pad's D-pad glyphs up, down, right, left (the PS2's order).
			float gx = column - leftW[3];
			for (int k = 0; k < nd; k++, gx += gs + sy * 2.0f)
				if (CFont::ButtonSprite[dpad[k]].m_pTexture)
					CFont::ButtonSprite[dpad[k]].Draw(CRect(gx, y + 0.5f * sy, gx + gs, y + 0.5f * sy + gs), glyphColor);
		} else {
			// Without the glyph pack: the PS2's four filled black arrows (spacing as measured),
			// ending at the column.
			static const float cx[4] = { 1078.0f, 1118.0f, 1157.0f, 1192.0f };
			const float cy = 873.0f, r = 10.0f, shift = column - HX(1202.0f);
			const float yshift = (y + gs / 2.0f) - HY(cy);	// centred on this row
			auto HY = [&](float v) { return gCornerY + SCREEN_SCALE_Y((v - 870.0f) * 448.0f / 1080.0f) + yshift; };
			CRGBA black(0, 0, 0, alpha);
			auto X = [&](float x) { return HX(x) + shift; };
			CSprite2d::Draw2DPolygon(X(cx[0]), HY(cy - r), X(cx[0]), HY(cy - r), X(cx[0] - r), HY(cy + r), X(cx[0] + r), HY(cy + r), black);
			CSprite2d::Draw2DPolygon(X(cx[1] - r), HY(cy - r), X(cx[1] + r), HY(cy - r), X(cx[1]), HY(cy + r), X(cx[1]), HY(cy + r), black);
			if (nd == 4) {
			CSprite2d::Draw2DPolygon(X(cx[2] - r), HY(cy - r), X(cx[2] + r), HY(cy), X(cx[2] - r), HY(cy + r), X(cx[2] + r), HY(cy), black);
			CSprite2d::Draw2DPolygon(X(cx[3] + r), HY(cy - r), X(cx[3] - r), HY(cy), X(cx[3] + r), HY(cy + r), X(cx[3] - r), HY(cy), black);
			}
		}
		CFont::PrintString(column + dashGap, y, TheText.Get("RVC_HDA"));
		CFont::PrintString(column + actionGap, y, TheText.Get(actions[i]));
	}
}

// Whether the skin draws on this page: PS2 UI style, a game running (the PS2 pause menu), a page
// that has a PS2 tab. The menu then shows fewer rows so the list stays above the help box.
bool
MenuSkinPS2Active(int page, bool gameNotLoaded)
{
	// REVC_TEST_MENU_SKIN: also before a game is loaded (screenshots from the boot test).
	static bool test = getenv("REVC_TEST_MENU_SKIN") != nil;
	// (the Save page has no tab, but the PS2 frame and help box too)
	return gUiStyle == 1 && (!gameNotLoaded || test) && (CurrentTab(page) >= 0 || page == MENUPAGE_CHOOSE_SAVE_SLOT ||
		page == MENUPAGE_SAVE_OVERWRITE_CONFIRM || page == MENUPAGE_SAVING_IN_PROGRESS || page == MENUPAGE_SAVE_SUCCESSFUL);
}

// Navigation as on the PS2 (measured: autopad probes in PCSX2, EU v3.00). Focus is on the tab bar
// or in the page's list:
// - the tab bar: left/right move in the row, up/down to the other row in the same column (both
//   wrap); the page changes at once; cross enters the list at row 0 (never remembered); triangle
//   closes the menu. L1/R1 do nothing.
// - the list: the PC menu's own input, except triangle, which goes back to the tab bar (on
//   pages that aren't a tab's own page, e.g. a confirmation, triangle goes back as on the PC,
//   and the tab page then has the tab bar focused).
// START from the game opens the Map page with the tab bar focused (Frontend.cpp).
// pageFocus: true = the list has focus.

bool
MenuSkinPS2PageFocus(void)
{
	return pageFocus;
}

// The Load page's list is two columns on the PS2: "load game" / "new game" / (revc: "console
// discs") on the left, then the slots. Cross from the tab bar focuses "load game"; cross on it
// moves into the slots, triangle there comes back to the left column. loadRow: 0 load game,
// 1 new game (entry 8), 2 console discs (entry 9).
// Audio: the "output" cell (beside radio station) has focus; m_nCurrOption is the radio row.
static bool audioOutput;
bool MenuSkinPS2AudioOutputFocus(void) { return audioOutput; }
// Audio edit mode: cross on a cell; the box grows over label and value, left/right change the
// value, triangle goes straight to the tab bar.
static bool audioEdit;
bool MenuSkinPS2AudioEdit(void) { return audioEdit; }

static bool loadColumn;
static int loadRow;

bool
MenuSkinPS2LoadHeadingFocus(CMenuManager *m)
{
	return pageFocus && loadColumn && loadRow == 0 && m->m_nCurrScreen == MENUPAGE_CHOOSE_LOAD_SLOT && MenuSkinPS2Active(m->m_nCurrScreen, m->m_bGameNotLoaded);
}

// The list has focus (test shots that select a row; Frontend.cpp's highlight checks).
void
MenuSkinPS2SetFocus(int page, bool list)
{
	focusPage = page;
	pageFocus = list;
}

// A tab's own page (the pages the tab bar switches to)?
static bool
IsTabPage(int page)
{
	for (int i = 0; i < (int)ARRAY_SIZE(tabs); i++)
		if (tabs[i].page == page)
			return true;
	return false;
}

bool
MenuSkinPS2TabFocus(CMenuManager *m)
{
	return MenuSkinPS2Active(m->m_nCurrScreen, m->m_bGameNotLoaded) && IsTabPage(m->m_nCurrScreen) && !pageFocus;
}

bool
MenuSkinPS2Input(CMenuManager *m)
{
	if (gScreenPositionAdjust) {
		CPad *p = CPad::GetPad(0);
		if (m->m_nCurrScreen != MENUPAGE_CONSOLE_DISPLAY || p->GetCrossJustDown() || p->GetTriangleJustDown() || p->GetEnterJustDown() ||
		    p->GetEscapeJustDown()) {
			gScreenPositionAdjust = false;
			m->SaveSettings();
			return true;
		}
		int x = (((uint8)gScreenPosition) >> 4) - 8, y = (((uint8)gScreenPosition) & 15) - 8;
		if (p->GetDPadLeftJustDown() || p->GetLeftJustDown()) x--;
		if (p->GetDPadRightJustDown() || p->GetRightJustDown()) x++;
		if (p->GetDPadUpJustDown() || p->GetUpJustDown()) y--;
		if (p->GetDPadDownJustDown() || p->GetDownJustDown()) y++;
		x = Clamp(x, -8, 7);
		y = Clamp(y, -8, 7);
		gScreenPosition = (int8)(((x + 8) << 4) | (y + 8));
		return true;
	}
	if (m->m_nCurrScreen != focusPage) {
		// a new page: a tab's page has the tab bar focused, any other page (confirmations,
		// sub-pages) its list
		focusPage = m->m_nCurrScreen;
		pageFocus = !IsTabPage(m->m_nCurrScreen);
		loadColumn = false;
		audioOutput = false;
		audioEdit = false;
	}
	if (!MenuSkinPS2Active(m->m_nCurrScreen, m->m_bGameNotLoaded))
		return false;
	CPad *pad = CPad::GetPad(0);
	const bool tri = pad->GetTriangleJustDown() || pad->GetEscapeJustDown();
	const bool cross = pad->GetCrossJustDown() || pad->GetEnterJustDown();
	const bool up = pad->GetDPadUpJustDown() || pad->GetUpJustDown(), down = pad->GetDPadDownJustDown() || pad->GetDownJustDown();
	if (pageFocus && m->m_nCurrScreen == MENUPAGE_CHOOSE_LOAD_SLOT) {
		int rows = aScreens[MENUPAGE_CHOOSE_LOAD_SLOT].m_aEntries[9].m_Action == MENUACTION_NOTHING ? 2 : 3;
		if (!loadColumn) {	// the slots
			if (tri) {
				loadColumn = true;
				loadRow = 0;
				return true;
			}
			return false;
		}
		if (tri) {
			pageFocus = false;
			loadColumn = false;
			return true;
		}
		if (up || down) {
			loadRow = (loadRow + (down ? 1 : rows - 1)) % rows;
			if (loadRow > 0)
				m->m_nCurrOption = 7 + loadRow;
			return true;
		}
		if (cross && loadRow == 0) {	// into the slots
			loadColumn = false;
			m->m_nCurrOption = 0;
			return true;
		}
		if (loadRow > 0) {
			m->m_nCurrOption = 7 + loadRow;
			return false;	// cross on new game / console discs: the PC menu acts on the row
		}
		return true;
	}
	if (pageFocus && m->m_nCurrScreen == MENUPAGE_SOUND_SETTINGS) {
		// Audio is a 2x2 grid on the PS2 (v1.40): music | SFX on top, radio station | output
		// below; left/right and up/down move between the cells and wrap on both axes. (How the
		// console changes a volume or the station isn't known yet: left/right only move here.)
		const bool left = pad->GetDPadLeftJustDown() || pad->GetLeftJustDown(), right = pad->GetDPadRightJustDown() || pad->GetRightJustDown();
		int music = -1, sfx = -1, radio = -1;
		for (int i = 0; i < NUM_MENUROWS; i++) {
			int a = aScreens[MENUPAGE_SOUND_SETTINGS].m_aEntries[i].m_Action;
			if (a == MENUACTION_MUSICVOLUME) music = i;
			if (a == MENUACTION_SFXVOLUME) sfx = i;
			if (a == MENUACTION_RADIO) radio = i;
		}
		if (audioEdit) {
			if (tri) {
				audioEdit = false;
				pageFocus = false;
				return true;
			}
			if ((left || right) && !audioOutput)
				return false;	// the PC menu changes the volume / station
			return true;
		}
		if (cross) {
			audioEdit = true;
			return true;
		}
		if (music >= 0 && sfx >= 0 && radio >= 0 && (up || down || left || right)) {
			int cell = m->m_nCurrOption == sfx ? 1 : m->m_nCurrOption == radio ? (audioOutput ? 3 : 2) : 0;
			if (left || right)
				cell ^= 1;
			else
				cell ^= 2;
			audioOutput = cell == 3;
			m->m_nCurrOption = cell == 0 ? music : cell == 1 ? sfx : radio;
			return true;
		}
	}
	if (pageFocus) {
		if (IsTabPage(m->m_nCurrScreen) && tri) {
			pageFocus = false;
			return true;
		}
		return false;
	}
	if (!IsTabPage(m->m_nCurrScreen))
		return false;
	// the tab bar
	if (pad->GetTriangleJustDown() || pad->GetEscapeJustDown()) {
		m->m_bShutDownFrontEndRequested = true;
		return true;
	}
	if (cross) {
		pageFocus = true;
		loadColumn = m->m_nCurrScreen == MENUPAGE_CHOOSE_LOAD_SLOT;
		loadRow = 0;
		audioOutput = false;
		audioEdit = false;
		m->m_nCurrOption = aScreens[m->m_nCurrScreen].m_aEntries[0].m_Action == MENUACTION_LABEL ? 1 : 0;
		m->m_nFirstVisibleRowOnList = 0;
		return true;
	}
	int n = ARRAY_SIZE(tabs), perRow = (n + 1) / 2;
	int cur = CurrentTab(m->m_nCurrScreen), row = cur / perRow, col = cur % perRow;
	int rowLen[2] = { perRow, n - perRow };
	if (pad->GetDPadLeftJustDown() || pad->GetLeftJustDown()) col = (col + rowLen[row] - 1) % rowLen[row];
	else if (pad->GetDPadRightJustDown() || pad->GetRightJustDown()) col = (col + 1) % rowLen[row];
	else if (pad->GetDPadUpJustDown() || pad->GetUpJustDown() || pad->GetDPadDownJustDown() || pad->GetDownJustDown()) {
		row = 1 - row;
		col = Min(col, rowLen[row] - 1);
	} else
		return true;	// nothing else acts on the tab bar
	int next = row * perRow + col;
	if (next != cur) {
		m->SwitchToNewScreen(SWITCH_TO_PAGE(tabs[next].page));
		focusPage = m->m_nCurrScreen;
		pageFocus = false;
	}
	return true;
}

// Called by CMenuManager::DrawBackground after the page; draws nothing unless UI STYLE is PS2.
void
MenuSkinPS2Draw(CMenuManager *m, uint8 alpha, float cornerX, float cornerY)
{
	// The help box sits at the same place on every page (console: about (438,303)-(626,376) at
	// 640x448), over the panel's bottom-right corner: placed from the Display page's corner.
	(void)cornerX; (void)cornerY;
	gCornerX = PX(1345.0f) + SX(5.0f);
	gCornerY = PY(870.0f) - SY(1.0f);	// (top edge at y 303 for x 560, 640x448: v3.00 captures)
	if (!MenuSkinPS2Active(m->m_nCurrScreen, m->m_bGameNotLoaded))
		return;
	DrawHelpBox(alpha, m->m_nCurrScreen);
	DrawTabs(m, alpha);
	CFont::DrawFonts();
}
