// revc: the Xbox visual style's per-model alpha test. The Xbox build gives some models their own
// alpha-test threshold: GREATEREQUAL with reference alpha x scale (alpha 255, or a fading entity's
// fade alpha), set around all draws of that entity. The scales are read once from the Xbox import
// (data/alpharef.txt, written by the importer from the player's own executable: "<model id> <scale>"
// per line). A model without a row, or any other visual style, keeps the alpha state as it is.
#include "common.h"

int VisualStyleIndex(void);	// (core/MenuScreensCustom.cpp: 2 = Xbox)
extern "C" ssize_t revc_import_read(int kind, const char *rel, uint8 *buf, size_t cap);

struct AlphaRefEntry {
	bool active;
	int savedFunc;
	int savedRef;
};

static AlphaRefEntry gStack[8];
static int gStackTop = 0;
static int gOverflow = 0;	// Begins that found the stack full (their Ends pop nothing)
static float gScale[MODELINFOSIZE] = { 0 };
static bool gLoaded = false;
static uint8 gFileBuf[256 * 1024];
static char gParseBuf[256 * 1024];

static void PushEntry(bool active, int savedFunc, int savedRef) {
	if (gStackTop >= 8) {
		gOverflow++;
		return;
	}
	{
		gStack[gStackTop].active = active;
		gStack[gStackTop].savedFunc = savedFunc;
		gStack[gStackTop].savedRef = savedRef;
		gStackTop++;
	}
}

static void PopEntry(void) {
	if (gOverflow > 0) {
		gOverflow--;
		return;
	}
	if (gStackTop > 0) {
		gStackTop--;
		if (gStack[gStackTop].active) {
			rw::SetRenderState(rw::ALPHATESTFUNC, gStack[gStackTop].savedFunc);
			rw::SetRenderState(rw::ALPHATESTREF, gStack[gStackTop].savedRef);
		}
	}
}

static void LoadTable(void) {
	ssize_t len = revc_import_read(0, "data/alpharef.txt", gFileBuf, sizeof(gFileBuf));
	if (len <= 0 || len >= (ssize_t)sizeof(gFileBuf)) {
		gLoaded = true;
		return;
	}
	memcpy(gParseBuf, gFileBuf, len);
	gParseBuf[len] = '\0';

	char *line = gParseBuf;
	while (line) {
		char *next = strchr(line, '\n');
		if (next) {
			*next = '\0';
			next++;
		}
		char *p = line;
		while (*p == ' ' || *p == '\t') p++;
		if (*p == '\0' || *p == '#') {
			line = next;
			continue;
		}
		char *endptr;
		long id = strtol(p, &endptr, 10);
		if (endptr == p) { line = next; continue; }
		p = endptr;
		while (*p == ' ' || *p == '\t') p++;
		double scale = strtod(p, &endptr);
		if (endptr == p) { line = next; continue; }
		if (id >= 0 && id < MODELINFOSIZE && scale > 0.0 && scale <= 1.0) {
			gScale[(int)id] = (float)scale;
		}
		line = next;
	}
	gLoaded = true;
}

void XboxAlphaRefBegin(int modelId, int alpha) {
	if (VisualStyleIndex() != 2) {
		PushEntry(false, 0, 0);
		return;
	}
	if (!gLoaded) LoadTable();

	int savedFunc = 0, savedRef = 0;
	bool active = false;

	if (gStackTop < 8 && modelId >= 0 && modelId < MODELINFOSIZE && gScale[modelId] > 0.0f) {
		int ref = (int)(Clamp(alpha, 0, 255) * gScale[modelId]);
		active = true;
		savedFunc = rw::GetRenderState(rw::ALPHATESTFUNC);
		savedRef = rw::GetRenderState(rw::ALPHATESTREF);
		rw::SetRenderState(rw::ALPHATESTFUNC, rw::ALPHAGREATEREQUAL);
		rw::SetRenderState(rw::ALPHATESTREF, ref);
	}
	PushEntry(active, savedFunc, savedRef);
}

void XboxAlphaRefEnd(void) {
	PopEntry();
}
