// revc (plan task 13): the Xbox pause menu's "tutorial briefings" (Brief) and "unlocked items"
// (Stats) lists, as the Xbox builds them (decoded from its default.xbe by the research session,
// xbox-research/MENU_ANIMATIONS.md):
//  - each list is defined by a file on the Xbox disc, neo/Hints.txt and neo/UnlockedItems.txt,
//    lifted by the disc import: lines "KEY [SHOWN_KEY]", '#' in column 1 for comments;
//  - every text label the mission script prints is offered to both lists; a key in the list's
//    file and not yet in the list is appended (oldest first), shown as SHOWN_KEY's text if given;
//  - one renderer: GXT text in the standard font, wrapped, 17 px apart in the blue card; "Nothing
//    yet..." when empty; longer than 156 px (9 lines) it scrolls in a loop with a 7-line gap,
//    fading at the edges; a handwritten "New" left of an entry until it has been shown once;
//  - the lists belong to the saved game. PC saves have no room for them, so revc keeps them in a
//    sidecar file next to the save (<save>.lists).
// Without an Xbox import the files are missing, nothing is ever added and the lists say "Nothing
// yet...".
#include "common.h"
#include "Frontend.h"
#include "Font.h"
#include "Sprite2d.h"
#include "Text.h"
#include "Timer.h"

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

extern "C" ssize_t revc_import_read(int kind, const char *rel, uint8 *buf, size_t cap);

enum { LIST_TUTORIAL, LIST_UNLOCKED, NUM_LISTS };
enum { MAX_DEFS = 256, MAX_ENTRIES = 128, KEY_LEN = 8, TEXT_LEN = 256 };

struct BriefList {
	bool defsRead;
	int numDefs;
	char defKey[MAX_DEFS][KEY_LEN], defShown[MAX_DEFS][KEY_LEN];
	int num;
	char key[MAX_ENTRIES][KEY_LEN], shown[MAX_ENTRIES][KEY_LEN];
	bool seen[MAX_ENTRIES];
	bool drawn[MAX_ENTRIES];	// drawn in the current visit: seen from the next visit on
	// the text as printed: mission texts live in mission tables that are not loaded later
	wchar text[MAX_ENTRIES][TEXT_LEN];
};
static BriefList lists[NUM_LISTS];
static const char *listFiles[NUM_LISTS] = { "neo/hints.txt", "neo/unlockeditems.txt" };

static void
ReadToken(const char *&p, char *out)
{
	while (*p == ' ' || *p == '\t')
		p++;
	int n = 0;
	while (*p && *p != ' ' && *p != '\t' && *p != '\r' && *p != '\n') {
		if (n < KEY_LEN - 1)
			out[n++] = *p;
		p++;
	}
	out[n] = '\0';
}

static void
ReadDefs(BriefList &l, int which)
{
	if (l.defsRead)
		return;
	l.defsRead = true;
	static uint8 buf[64 * 1024];
	ssize_t len = revc_import_read(0, listFiles[which], buf, sizeof(buf) - 1);
	if (len <= 0)
		return;
	buf[len] = '\0';
	for (const char *p = (const char *)buf; *p && l.numDefs < MAX_DEFS;) {
		if (*p != '#') {
			char a[KEY_LEN], b[KEY_LEN];
			ReadToken(p, a);
			ReadToken(p, b);
			if (a[0]) {
				strcpy(l.defKey[l.numDefs], a);
				strcpy(l.defShown[l.numDefs], b[0] ? b : a);
				l.numDefs++;
			}
		}
		while (*p && *p != '\n')
			p++;
		if (*p)
			p++;
	}
}

// A text label the mission script reads to print (CTheScripts::ReadTextLabelFromScript,
// GetTextByKeyFromScript): appended to each list whose file names it.
void
BriefListsOffer(const char *label)
{
	char k[KEY_LEN];
	strncpy(k, label, KEY_LEN - 1);
	k[KEY_LEN - 1] = '\0';
	for (int w = 0; w < NUM_LISTS; w++) {
		BriefList &l = lists[w];
		ReadDefs(l, w);
		for (int d = 0; d < l.numDefs; d++) {
			if (strcmp(l.defKey[d], k) != 0)
				continue;
			bool have = false;
			for (int e = 0; e < l.num && !have; e++)
				have = strcmp(l.key[e], k) == 0;
			if (!have && l.num < MAX_ENTRIES) {
				strcpy(l.key[l.num], k);
				strcpy(l.shown[l.num], l.defShown[d]);
				wchar *t = TheText.Get(l.defShown[d]);
				int n = 0;
				for (; t[n] && n < TEXT_LEN - 1; n++)
					l.text[l.num][n] = t[n];
				l.text[l.num][n] = 0;
				l.seen[l.num] = false;
				l.drawn[l.num] = false;
				l.num++;
			}
			break;
		}
	}
}

// A new game (CTheScripts::Init): empty lists.
void
BriefListsReset(void)
{
	for (int w = 0; w < NUM_LISTS; w++)
		lists[w].num = 0;
}

// The sidecar next to a save: "RVBL", version 1, then per list a count and its entries (key,
// shown key, seen flag, the text: u16 length + characters).
static void
SidecarName(const char *save, char *out, size_t size)
{
	snprintf(out, size, "%s.lists", save);
}

void
BriefListsSave(const char *saveFile)
{
	char name[300];
	SidecarName(saveFile, name, sizeof(name));
	FILE *f = fopen(name, "wb");
	if (f == nil)
		return;
	fwrite("RVBL\2", 1, 5, f);
	for (int w = 0; w < NUM_LISTS; w++) {
		uint8 n = lists[w].num;
		fwrite(&n, 1, 1, f);
		for (int e = 0; e < n; e++) {
			fwrite(lists[w].key[e], 1, KEY_LEN, f);
			fwrite(lists[w].shown[e], 1, KEY_LEN, f);
			uint8 s = lists[w].seen[e];
			fwrite(&s, 1, 1, f);
			uint16 n = 0;
			while (n < TEXT_LEN - 1 && lists[w].text[e][n])
				n++;
			fwrite(&n, 2, 1, f);
			fwrite(lists[w].text[e], sizeof(wchar), n, f);
		}
	}
	fclose(f);
}

void
BriefListsLoad(const char *saveFile)
{
	BriefListsReset();
	char name[300];
	SidecarName(saveFile, name, sizeof(name));
	FILE *f = fopen(name, "rb");
	if (f == nil)
		return;
	char magic[5];
	if (fread(magic, 1, 5, f) == 5 && memcmp(magic, "RVBL\2", 5) == 0) {
		for (int w = 0; w < NUM_LISTS; w++) {
			uint8 n = 0;
			if (fread(&n, 1, 1, f) != 1)
				break;
			for (int e = 0; e < n && e < MAX_ENTRIES; e++) {
				uint8 s = 0;
				if (fread(lists[w].key[e], 1, KEY_LEN, f) != KEY_LEN || fread(lists[w].shown[e], 1, KEY_LEN, f) != KEY_LEN ||
				    fread(&s, 1, 1, f) != 1)
					break;
				lists[w].key[e][KEY_LEN - 1] = lists[w].shown[e][KEY_LEN - 1] = '\0';
				uint16 n = 0;
				if (fread(&n, 2, 1, f) != 1 || n >= TEXT_LEN || fread(lists[w].text[e], sizeof(wchar), n, f) != n)
					break;
				lists[w].text[e][n] = 0;
				lists[w].seen[e] = s != 0;
				lists[w].drawn[e] = false;
				lists[w].num = e + 1;
			}
		}
	}
	fclose(f);
}

// Draws list `which` (0 tutorial briefings, 1 unlocked items) in the card x0..x1 (menu units,
// 640 wide) with its 156 px window centred on cy (448-line menu units), as the Xbox does.
void
BriefListsDraw(int which, float x0, float x1, float cy, uint8 alpha)
{
	BriefList &l = lists[which];
	// a new visit (not drawn for a moment): what was shown last time is no longer "New"
	static uint32 lastDraw[NUM_LISTS];
	uint32 now = CTimer::GetTimeInMillisecondsPauseMode();
	if (now - lastDraw[which] > 250)
		for (int e = 0; e < l.num; e++) {
			if (l.drawn[e])
				l.seen[e] = true;
			l.drawn[e] = false;
		}
	lastDraw[which] = now;
	// 17 px apart, the font's own line spacing for wrapped lines (measured in the shots)
	const float pitch = 17.0f, window = 156.0f;
	CFont::SetFontStyle(FONT_LOCALE(FONT_STANDARD));
	CFont::SetScale(MENU_X(MEDIUMTEXT_X_SCALE), MENU_Y(MEDIUMTEXT_Y_SCALE));
	CFont::SetPropOn();
	CFont::SetDropShadowPosition(0);
	CFont::SetRightJustifyOff();
	CFont::SetBackgroundOff();
	if (l.num == 0) {
		CFont::SetCentreOn();
		CFont::SetCentreSize(SCREEN_WIDTH);
		CFont::SetColor(CRGBA(0, 0, 0, alpha));
		CFont::PrintString(MENU_X_LEFT_ALIGNED((x0 + x1) / 2.0f), MENU_Y(cy - pitch / 2.0f), TheText.Get("RVC_NIY"));
		CFont::SetCentreOff();
		return;
	}
	CFont::SetCentreOff();
	CFont::SetWrapx(MENU_X_LEFT_ALIGNED(x1));
	// heights (lines) of the entries
	int lines[MAX_ENTRIES], total = 0;
	for (int e = 0; e < l.num; e++) {
		lines[e] = Max(1, (int)CFont::GetNumberLines(MENU_X_LEFT_ALIGNED(x0), 0.0f, l.text[e]));
		total += lines[e];
	}
	float height = total * pitch, top = cy - window / 2.0f, start = top, loop = 0.0f;
	if (height <= window)
		start = cy - height / 2.0f;	// fits: centred
	else {
		loop = height + 7.0f * pitch;	// scrolls in a loop, a 7-line gap before it repeats
		start = top + window - fmodf(CTimer::GetTimeInMillisecondsPauseMode() * 0.02f, loop);
	}
	for (int pass = 0; pass < (loop > 0.0f ? 2 : 1); pass++) {
		float y = start + pass * loop;
		for (int e = 0; e < l.num; e++, y += lines[e - 1] * pitch) {
			float h = lines[e] * pitch;
			if (loop > 0.0f && (y + h < top || y > top + window))
				continue;
			// fade at the window's edges (scrolling lists)
			float a = 1.0f;
			if (loop > 0.0f)
				a = Clamp(Min(y - top, top + window - (y + pitch)) / pitch + 1.0f, 0.0f, 1.0f);
			CFont::SetColor(CRGBA(0, 0, 0, alpha * a));
			CFont::PrintString(MENU_X_LEFT_ALIGNED(x0), MENU_Y(y), l.text[e]);
			l.drawn[e] = true;
			if (!l.seen[e]) {
				// "New" in the handwriting font, shadow then pink, left of the first line
				CFont::SetFontStyle(FONT_LOCALE(FONT_BANK));
				CFont::SetScale(MENU_X(0.72f), MENU_Y(1.1f));
				CFont::SetColor(CRGBA(30, 30, 30, alpha * a));
				CFont::PrintString(MENU_X_LEFT_ALIGNED(x0 - 45.0f + 1.5f), MENU_Y(y - 5.0f + 1.5f), TheText.Get("RVC_NEW"));
				CFont::SetColor(CRGBA(255, 150, 225, alpha * a));
				CFont::PrintString(MENU_X_LEFT_ALIGNED(x0 - 45.0f), MENU_Y(y - 5.0f), TheText.Get("RVC_NEW"));
				CFont::SetFontStyle(FONT_LOCALE(FONT_STANDARD));
				CFont::SetScale(MENU_X(MEDIUMTEXT_X_SCALE), MENU_Y(MEDIUMTEXT_Y_SCALE));
			}
		}
	}
	CFont::DrawFonts();
}

// Test: entries for screenshots without playing (REVC_TEST_BRIEF_LISTS=KEY,KEY,...), offered as
// the script would.
extern "C" void	// (C linkage: called from skel/host/testhooks.cpp)
BriefListsTestFill(void)
{
	const char *keys = getenv("REVC_TEST_BRIEF_LISTS");
	if (keys == nil)
		return;
	char k[KEY_LEN];
	for (const char *p = keys; *p;) {
		int n = 0;
		while (*p && *p != ',') {
			if (n < KEY_LEN - 1)
				k[n++] = *p;
			p++;
		}
		k[n] = '\0';
		if (n)
			BriefListsOffer(k);
		if (*p)
			p++;
	}
}
