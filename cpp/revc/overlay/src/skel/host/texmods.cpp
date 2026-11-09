// Texture mods, C++ half: swap rasters of loaded textures for images supplied by the Rust host
// (rust/revc/src/texmods.rs indexes mods/textures and decodes PNG/DDS to RGBA8).
#if defined LIBRW_HOST

#include "common.h"
#include "rwcore.h"
#include "TexMods.h"

extern "C" {
// Non-zero if the mods folder holds any texture overrides (skips per-texture lookups otherwise).
int revc_texmod_active(void);
// If an override for txd/texture exists and decodes, returns 1 and an RGBA8 image, top row first
// (librw's image convention; checked by the texmods test). Free it with revc_texmod_free.
int revc_texmod_find(const char *txd, const char *texture, int *width, int *height, unsigned char **rgba);
void revc_texmod_free(unsigned char *rgba, int width, int height);
}

static rw::Texture *
ApplyOne(rw::Texture *tex, void *data)
{
	const char *txdName = (const char*)data;
	int w, h;
	unsigned char *px;
	if (!revc_texmod_find(txdName, tex->name, &w, &h, &px))
		return tex;

	rw::Raster *old = tex->raster;
	bool mipmapped = (old && (old->format & rw::Raster::MIPMAP)) ||
	                 tex->getFilter() >= rw::Texture::MIPNEAREST;
	int32 format = rw::Raster::C8888 | rw::Raster::TEXTURE;
	if (mipmapped)
		format |= rw::Raster::MIPMAP | rw::Raster::AUTOMIPMAP;

	rw::Image *img = rw::Image::create(w, h, 32);
	img->pixels = px;	// not owned by the image (flags stay 0), freed by Rust below
	img->stride = w * 4;
	rw::Raster *ras = rw::Raster::create(w, h, 32, format);
	if (ras && ras->setFromImage(img)) {
		if (old)
			old->destroy();
		tex->raster = ras;
	} else {
		debug("texmods: couldn't create a %dx%d raster for %s/%s, keeping the original\n", w, h, txdName, tex->name);
		if (ras)
			ras->destroy();
	}
	img->pixels = nil;
	img->destroy();
	revc_texmod_free(px, w, h);
	return tex;
}

void
TexModsApply(RwTexDictionary *dict, const char *txdName)
{
	if (dict == nil || !revc_texmod_active())
		return;
	RwTexDictionaryForAllTextures(dict, (RwTextureCallBack)ApplyOne, (void*)txdName);
}

#endif
