#pragma once
// revc: the Xbox visual style's per-model alpha test (XboxAlphaRef.cpp).
// Begin/End around all draws of one entity (they nest); alpha is 255, or a fading entity's fade alpha.
void XboxAlphaRefBegin(int modelId, int alpha);
void XboxAlphaRefEnd(void);

struct XboxAlphaRefScope
{
	XboxAlphaRefScope(int modelId, int alpha) { XboxAlphaRefBegin(modelId, alpha); }
	~XboxAlphaRefScope(void) { XboxAlphaRefEnd(); }
};
