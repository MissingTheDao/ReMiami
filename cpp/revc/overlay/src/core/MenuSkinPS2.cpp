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
	{ MENUPAGE_CONTROLLER_PC, -1, 332.0f, 995.0f, "RVC_PT4" },
	{ MENUPAGE_SOUND_SETTINGS, -1, 568.0f, 995.0f, "RVC_PT5" },
	{ MENUPAGE_DISPLAY_SETTINGS, MENUPAGE_GRAPHICS_SETTINGS, 785.0f, 995.0f, "RVC_PT6" },
	{ MENUPAGE_LANGUAGE_SETTINGS, -1, 1043.0f, 995.0f, "RVC_PT7" },
};

static int
CurrentTab(int page)
{
	for (int i = 0; i < (int)ARRAY_SIZE(tabs); i++)
		if (tabs[i].page == page || tabs[i].alt == page)
			return i;
	return -1;
}

static void
DrawTabs(CMenuManager *m, uint8 alpha)
{
	int cur = CurrentTab(m->m_nCurrScreen);
	if (cur < 0)
		return;
	// The active tab's slanted box (measured around DISPLAY: 672..890 x 950..1060), moved to the
	// current tab.
	const Ps2Tab &t = tabs[cur];
	float dx = t.x - 785.0f, dy = t.y - 995.0f;
	CSprite2d::Draw2DPolygon(PX(672.0f + dx), PY(958.0f + dy), PX(895.0f + dx), PY(948.0f + dy),
		PX(672.0f + dx), PY(1062.0f + dy), PX(885.0f + dx), PY(1052.0f + dy),
		CRGBA(PS2_GREEN.r, PS2_GREEN.g, PS2_GREEN.b, alpha));

	CFont::SetFontStyle(FONT_LOCALE(FONT_HEADING));
	CFont::SetScale(SX(0.55f), SY(0.75f));
	CFont::SetCentreOn();
	CFont::SetCentreSize(SCREEN_WIDTH);
	CFont::SetPropOn();
	CFont::SetBackgroundOff();
	CFont::SetDropShadowPosition(2);
	CFont::SetDropColor(CRGBA(0, 0, 0, alpha));
	CFont::SetColor(CRGBA(PS2_TAB_TEXT.r, PS2_TAB_TEXT.g, PS2_TAB_TEXT.b, alpha));
	for (int i = 0; i < (int)ARRAY_SIZE(tabs); i++)
		CFont::PrintString(PX(tabs[i].x), PY(tabs[i].y - 24.0f), TheText.Get(tabs[i].name));
	CFont::SetCentreOff();
	CFont::SetDropShadowPosition(0);
}

static void
DrawHelpBox(uint8 alpha)
{
	// thick black outline, about 8 capture pixels all round (measured)
	const float o = 8.0f;
	CSprite2d::Draw2DPolygon(HX(975.0f - o), HY(742.0f - o), HX(1390.0f + o), HY(728.0f - o), HX(990.0f - o), HY(882.0f + o), HX(1418.0f + o), HY(910.0f + o),
		CRGBA(0, 0, 0, alpha));
	CSprite2d::Draw2DPolygon(HX(975.0f), HY(742.0f), HX(1390.0f), HY(728.0f), HX(990.0f), HY(882.0f), HX(1418.0f), HY(910.0f),
		CRGBA(PS2_GREEN.r, PS2_GREEN.g, PS2_GREEN.b, alpha));
	// Select/Back: the pad's buttons as outline-only glyphs in black, like the text (the PS2 UI
	// style loads the glyph pack's outline variant, white line art, tinted here). Without the
	// pack the game's own coloured button textures are drawn as they are.
	static const int glyphs[2] = { BUTTON_CROSS, BUTTON_TRIANGLE };
	static const char *keys[4] = { "", "", "RVC_HST", nil };
	static const char *actions[4] = { "RVC_HSE", "RVC_HBA", "RVC_HRE", "RVC_HNA" };
	CFont::SetFontStyle(FONT_LOCALE(FONT_STANDARD));
	CFont::SetScale(SX(0.41f), SY(0.68f));
	CFont::SetPropOn();
	CFont::SetBackgroundOff();
	CFont::SetDropShadowPosition(0);
	CFont::SetColor(CRGBA(0, 0, 0, alpha));	// black text on the green, as on the PS2
	wchar buf[64];
	for (int i = 0; i < 4; i++) {
		float y = HY(752.0f + 36.0f * i);
		// "<button> button" right-aligned before the dash, the action after it.
		if (keys[i] == nil)
			buf[0] = 0;	// the arrows, drawn below
		else if (keys[i][0] == 0)
			buf[0] = 0;
		else
			UnicodeStrcpy(buf, TheText.Get(keys[i]));
		if (i < 3) {
			int n = UnicodeStrlen(buf);
			if (n > 0)
				buf[n++] = ' ';
			UnicodeStrcpy(buf + n, TheText.Get("RVC_HBT"));
		}
		if (i < 2) {
			float sy = CFont::Details.scaleY;
			float w = CFont::GetStringWidth(buf, true);
			// a little larger than the font's glyph cell, like the PS2's symbols
			float gs = 21.0f * sy;
			float gx = HX(1205.0f) - w - sy * 4.0f - gs;
			CSprite2d &g = CFont::ButtonSprite[glyphs[i]];
			if (g.m_pTexture)
				g.Draw(CRect(gx, y + 0.5f * sy, gx + gs, y + 0.5f * sy + gs),
					gGlyphPackLoaded ? CRGBA(0, 0, 0, alpha) : CRGBA(255, 255, 255, alpha));
		}
		if (i < 3) {
			CFont::SetRightJustifyOn();
			CFont::PrintString(HX(1205.0f), y, buf);
			CFont::SetRightJustifyOff();
		} else {
			// The PS2 shows four filled black arrows here (not pad buttons): up, down, right,
			// left, centred at these capture pixels.
			static const float cx[4] = { 1078.0f, 1118.0f, 1157.0f, 1192.0f };
			const float cy = 873.0f, r = 10.0f;
			CRGBA black(0, 0, 0, alpha);
			CSprite2d::Draw2DPolygon(HX(cx[0]), HY(cy - r), HX(cx[0]), HY(cy - r), HX(cx[0] - r), HY(cy + r), HX(cx[0] + r), HY(cy + r), black);
			CSprite2d::Draw2DPolygon(HX(cx[1] - r), HY(cy - r), HX(cx[1] + r), HY(cy - r), HX(cx[1]), HY(cy + r), HX(cx[1]), HY(cy + r), black);
			CSprite2d::Draw2DPolygon(HX(cx[2] - r), HY(cy - r), HX(cx[2] + r), HY(cy), HX(cx[2] - r), HY(cy + r), HX(cx[2] + r), HY(cy), black);
			CSprite2d::Draw2DPolygon(HX(cx[3] + r), HY(cy - r), HX(cx[3] - r), HY(cy), HX(cx[3] + r), HY(cy + r), HX(cx[3] - r), HY(cy), black);
		}
		CFont::PrintString(HX(1215.0f), y, TheText.Get("RVC_HDA"));
		CFont::PrintString(HX(1245.0f), y, TheText.Get(actions[i]));
	}
}

// Whether the skin draws on this page: PS2 UI style, a game running (the PS2 pause menu), a page
// that has a PS2 tab. The menu then shows fewer rows so the list stays above the help box.
bool
MenuSkinPS2Active(int page, bool gameNotLoaded)
{
	// REVC_TEST_MENU_SKIN: also before a game is loaded (screenshots from the boot test).
	static bool test = getenv("REVC_TEST_MENU_SKIN") != nil;
	return gUiStyle == 1 && (!gameNotLoaded || test) && CurrentTab(page) >= 0;
}

// L1/R1 step through the tabs, as on the PS2 pause menu (the PC menu's own navigation stays).
// Returns true when it switched pages (the rest of this frame's menu input is skipped).
bool
MenuSkinPS2Input(CMenuManager *m)
{
	if (!MenuSkinPS2Active(m->m_nCurrScreen, m->m_bGameNotLoaded))
		return false;
	CPad *pad = CPad::GetPad(0);
	int step = pad->GetRightShoulder1JustDown() ? 1 : pad->GetLeftShoulder1JustDown() ? -1 : 0;
	if (step == 0)
		return false;
	int n = ARRAY_SIZE(tabs);
	int next = (CurrentTab(m->m_nCurrScreen) + step + n) % n;
	m->SwitchToNewScreen(tabs[next].page);
	return true;
}

// Called by CMenuManager::DrawBackground after the page; draws nothing unless UI STYLE is PS2.
void
MenuSkinPS2Draw(CMenuManager *m, uint8 alpha, float cornerX, float cornerY)
{
	gCornerX = cornerX;
	gCornerY = cornerY;
	if (!MenuSkinPS2Active(m->m_nCurrScreen, m->m_bGameNotLoaded))
		return;
	DrawHelpBox(alpha);
	DrawTabs(m, alpha);
	CFont::DrawFonts();
}
