// Texture mods, C++ half: swap rasters of loaded textures for images supplied by the Rust host
// (rust/revc/src/texmods.rs indexes mods/textures; DXT DDS files stay compressed, the rest is
// decoded to RGBA8).
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
// A DXT DDS override as its blocks (all levels one after the other), when it can stay
// compressed; free with revc_texmod_free_blocks. See rust/revc/src/texmods.rs.
int revc_texmod_find_dxt(const char *txd, const char *texture, int want_mips, int *width, int *height,
	int *kind, int *levels, int *has_alpha, unsigned char **blocks, size_t *size);
void revc_texmod_free_blocks(unsigned char *blocks, size_t size);
// The PS2 world pack's texture names for a dictionary (newline-separated), freed with
// revc_texmod_free_names; nil while the pack isn't in use.
char *revc_texmod_world_names(const char *txd);
int revc_texmod_from_world(const char *txd, const char *texture);
void revc_texmod_free_names(char *names);
}

// A raster holding the DXT blocks as they are (a BC texture on the GPU, like the game's own DXT
// textures after Raster::convertTexToCurrentPlatform). nil if it can't be made.
static rw::Raster *
DxtRaster(int w, int h, int kind, int levels, bool hasAlpha, const unsigned char *blocks, size_t size)
{
	// The 16-bit format D3D reports for such a texture (alpha from it, as for the originals).
	int32 format = kind == 1 ? (hasAlpha ? rw::Raster::C1555 : rw::Raster::C565) : rw::Raster::C4444;
	if (levels > 1)
		format |= rw::Raster::MIPMAP;
	rw::Raster *ras = rw::Raster::create(w, h, 16, format | rw::Raster::TEXTURE | rw::Raster::DONTALLOCATE);
	if (ras == nil)
		return nil;
	ras->format = format;	// rasterCreate stored RGBA8
	rw::wgpu::allocateDXT(ras, kind, levels, hasAlpha);
	size_t off = 0;
	for (int i = 0; i < ras->getNumLevels(); i++) {
		int32 lw = w >> i, lh = h >> i;
		size_t n = rw::wgpu::dxtLevelSize(kind, lw < 1 ? 1 : lw, lh < 1 ? 1 : lh);
		uint8 *dst = ras->lock(i, rw::Raster::LOCKWRITE | rw::Raster::LOCKNOFETCH);
		if (dst == nil || off + n > size) {
			if (dst)
				ras->unlock(i);
			ras->destroy();
			return nil;
		}
		memcpy(dst, blocks + off, n);
		off += n;
		ras->unlock(i);
	}
	return ras;
}

// Replaces tex's raster with the override for txdName/texName, if there is one.
static rw::Texture *
ApplyNamed(rw::Texture *tex, const char *txdName, const char *texName)
{
	int w, h;
	unsigned char *px;
	rw::Raster *old = tex->raster;
	bool mipmapped = (old && (old->format & rw::Raster::MIPMAP)) ||
	                 tex->getFilter() >= rw::Texture::MIPNEAREST;
	int32 format = rw::Raster::C8888 | rw::Raster::TEXTURE;
	// (the PS2 world textures keep the original's mip state: texmods.rs)
	if (mipmapped && old && !(old->format & rw::Raster::MIPMAP) && revc_texmod_from_world(txdName, texName))
		mipmapped = false;
	if (mipmapped)
		format |= rw::Raster::MIPMAP | rw::Raster::AUTOMIPMAP;

	int kind, levels, hasAlpha;
	unsigned char *blocks;
	size_t size;
	if (revc_texmod_find_dxt(txdName, texName, mipmapped, &w, &h, &kind, &levels, &hasAlpha, &blocks, &size)) {
		rw::Raster *ras = DxtRaster(w, h, kind, levels, hasAlpha != 0, blocks, size);
		revc_texmod_free_blocks(blocks, size);
		if (ras) {
			if (old)
				old->destroy();
			tex->raster = ras;
			return tex;
		}
		debug("texmods: couldn't create a %dx%d DXT raster for %s/%s, decoding it instead\n", w, h, txdName, texName);
	}
	if (!revc_texmod_find(txdName, texName, &w, &h, &px))
		return tex;

	rw::Image *img = rw::Image::create(w, h, 32);
	img->pixels = px;	// not owned by the image (flags stay 0), freed by Rust below
	img->stride = w * 4;
	rw::Raster *ras = rw::Raster::create(w, h, 32, format);
	if (ras && ras->setFromImage(img)) {
		if (old)
			old->destroy();
		tex->raster = ras;
	} else {
		debug("texmods: couldn't create a %dx%d raster for %s/%s, keeping the original\n", w, h, txdName, texName);
		if (ras)
			ras->destroy();
	}
	img->pixels = nil;
	img->destroy();
	revc_texmod_free(px, w, h);
	return tex;
}

static rw::Texture *
ApplyOne(rw::Texture *tex, void *data)
{
	return ApplyNamed(tex, (const char*)data, tex->name);
}

void
TexModsApplyTexture(RwTexture *tex, const char *txdName, const char *texName)
{
	if (tex && revc_texmod_active())
		ApplyNamed(tex, txdName, texName);
}

void
TexModsApply(RwTexDictionary *dict, const char *txdName)
{
	if (dict == nil || !revc_texmod_active())
		return;
	RwTexDictionaryForAllTextures(dict, (RwTextureCallBack)ApplyOne, (void*)txdName);
	// the PS2 models (rust/revc/src/ps2models.rs) use some textures that only the PS2's
	// dictionaries have: added from the PS2 world pack (linear, mipmapped, wrapping)
	if (char *names = revc_texmod_world_names(txdName)) {
		for (char *n = strtok(names, "\n"); n; n = strtok(nil, "\n")) {
			if (RwTexDictionaryFindNamedTexture(dict, n))
				continue;
			rw::Raster *ras = rw::Raster::create(1, 1, 32, rw::Raster::C8888 | rw::Raster::TEXTURE);
			rw::Texture *tex = rw::Texture::create(ras);
			strncpy(tex->name, n, 32);
			tex->filterAddressing = 0x1106;
			ApplyNamed(tex, txdName, n);
			RwTexDictionaryAddTexture(dict, tex);
		}
		revc_texmod_free_names(names);
	}
}

#endif
