#pragma once
// revc: the Rust scene functions (rust/revc/src/rwscene.rs), shared by clump.cpp, light.cpp,
// world.cpp and camera.cpp
namespace rw {
// revc (docs/RENDERER_PORT.md, stage 2 step 6b): clumps, atomics, lights, worlds and the camera
// object are made and kept in Rust (rust/revc/src/rwscene.rs), checked against
// tests/golden/rwscene.txt. Memory and plugin data stay here; the hooks are installed by
// camera.cpp, which owns the camera's device callbacks.
extern "C" {
Clump *revc_clump_create(void);
Clump *revc_clump_clone(Clump *c);
void revc_clump_destroy(Clump *c);
void revc_clump_add_atomic(Clump *c, Atomic *a);
void revc_clump_remove_atomic(Clump *c, Atomic *a);
void revc_clump_add_light(Clump *c, Light *l);
void revc_clump_remove_light(Clump *c, Light *l);
void revc_clump_add_camera(Clump *c, Camera *k);
void revc_clump_remove_camera(Clump *c, Camera *k);
void revc_clump_render(Clump *c);
Atomic *revc_atomic_create(void);
Atomic *revc_atomic_clone(Atomic *a);
void revc_atomic_destroy(Atomic *a);
void revc_atomic_set_geometry(Atomic *a, Geometry *g, uint32 flags);
Sphere *revc_atomic_get_world_bounding_sphere(Atomic *a);
Light *revc_light_create(int32 type);
void revc_light_destroy(Light *l);
void revc_light_set_angle(Light *l, float32 a);
float32 revc_light_get_angle(const Light *l);
void revc_light_set_color(Light *l, float32 r, float32 g, float32 b);
World *revc_world_create(void);
void revc_world_destroy(World *w);
void revc_world_add_light(World *w, Light *l);
void revc_world_remove_light(World *w, Light *l);
void revc_world_add_camera(World *w, Camera *k);
void revc_world_remove_camera(World *w, Camera *k);
void revc_world_add_atomic(World *w, Atomic *a);
void revc_world_remove_atomic(World *w, Atomic *a);
void revc_world_add_clump(World *w, Clump *c);
void revc_world_remove_clump(World *w, Clump *c);
void revc_world_render(World *w);
void revc_world_enumerate_lights(World *w, Atomic *a, WorldLights *ld);
Camera *revc_camera_create(void);
Camera *revc_camera_clone(Camera *k);
void revc_camera_destroy(Camera *k);
}
}
