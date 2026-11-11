#pragma once
// revc: the Rust plugin data functions (rust/revc/src/rwplugins.rs), shared by hanim.cpp,
// anim.cpp, skin.cpp and matfx.cpp
namespace rw {
// revc (docs/RENDERER_PORT.md, stage 2 step 6c): the plugin data is in Rust
// (rust/revc/src/rwplugins.rs), checked against tests/golden/rwplugins.txt.
extern "C" {
AnimInterpolator *revc_interp_create(int32 n, int32 size);
void revc_interp_destroy(AnimInterpolator *it);
HAnimHierarchy *revc_hier_create(int32 n, int32 *flags, int32 *ids, int32 hflags, int32 maxkey);
void revc_hier_destroy(HAnimHierarchy *h);
void revc_hier_attach_by_index(HAnimHierarchy *h, int32 i);
void revc_hier_attach(HAnimHierarchy *h);
int32 revc_hier_get_index_id(const HAnimHierarchy *h, int32 id);
int32 revc_hier_get_index_frame(const HAnimHierarchy *h, Frame *f);
HAnimHierarchy *revc_hier_find(Frame *f);
void revc_hanim_create(void *o);
void revc_hanim_destroy(void *o);
void revc_hanim_copy(void *d, void *s);
void revc_skin_create(void *g);
void revc_skin_destroy(void *g);
void revc_skin_copy(void *d, void *s);
void revc_skin_init(Skin *s, int32 nb, int32 nub, int32 nv);
void revc_skin_find_num_weights(Skin *s, int32 nv);
void revc_skin_find_used_bones(Skin *s, int32 nv);
void revc_matfx_atomic_create(void *a);
void revc_matfx_atomic_copy(void *d, void *s);
void revc_matfx_set_effects(Material *m, uint32 kind);
uint32 revc_matfx_get_effects(const Material *m);
int32 revc_matfx_effect_index(const MatFX *fx, uint32 kind);
void revc_matfx_set_bump_texture(MatFX *fx, Texture *t);
void revc_matfx_set_bump_coefficient(MatFX *fx, float32 c);
Texture *revc_matfx_get_bump_texture(MatFX *fx);
float32 revc_matfx_get_bump_coefficient(MatFX *fx);
void revc_matfx_set_env_texture(MatFX *fx, Texture *t);
void revc_matfx_set_env_frame(MatFX *fx, Frame *f);
void revc_matfx_set_env_coefficient(MatFX *fx, float32 c);
void revc_matfx_set_env_fb_alpha(MatFX *fx, int32 a);
Texture *revc_matfx_get_env_texture(MatFX *fx);
Frame *revc_matfx_get_env_frame(MatFX *fx);
float32 revc_matfx_get_env_coefficient(MatFX *fx);
int32 revc_matfx_get_env_fb_alpha(MatFX *fx);
void revc_matfx_set_dual_texture(MatFX *fx, Texture *t);
void revc_matfx_set_dual_src_blend(MatFX *fx, int32 b);
void revc_matfx_set_dual_dest_blend(MatFX *fx, int32 b);
Texture *revc_matfx_get_dual_texture(MatFX *fx);
int32 revc_matfx_get_dual_src_blend(MatFX *fx);
int32 revc_matfx_get_dual_dest_blend(MatFX *fx);
void revc_matfx_set_uv_transform(MatFX *fx, Matrix *b, Matrix *d);
void revc_matfx_get_uv_transform(MatFX *fx, Matrix **b, Matrix **d);
void revc_matfx_material_create(void *m);
void revc_matfx_material_destroy(void *m);
void revc_matfx_material_copy(void *d, void *s);
}
// sends the plugin offsets known so far to Rust (each register*Plugin calls it)
void revcPluginsUpdateHooks(void);
}
