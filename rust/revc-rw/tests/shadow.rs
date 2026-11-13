//! The retro-wgpu back end against the wgpu back end on one recorded frame: Im2D (textured,
//! blended, alpha-tested, fogged), Im3D, a lit mesh with point and directional lights, a skinned
//! mesh, a MatFX environment map, render-to-texture and a frame copy. Both images must pass
//! the shadow gate (99.5 % of pixels within 2 levels).

use revc_rw::frame::*;
use revc_rw::shadow::{GATE, Shadow, compare};
use revc_rw::{Backend, Gpu};

const SIZE: u32 = 64;

fn base() -> DrawState {
    DrawState {
        filter: 2,
        address_u: 1,
        address_v: 1,
        src_blend: 5,
        dst_blend: 6,
        cull: 1,
        alpha_func: 1,
        alpha_ref: 10.0 / 255.0,
        z_test: 1,
        z_write: 1,
        fog_start: 1.0,
        fog_end: 10.0,
        fog_color: 0xFF80_4020,
        xform: [2.0 / SIZE as f32, -2.0 / SIZE as f32, -1.0, 1.0],
        ..Default::default()
    }
}

fn quad(x0: f32, y0: f32, x1: f32, y1: f32, w: f32, rgba: [u8; 4]) -> [Vertex2D; 4] {
    let v = |x, y, u, v| Vertex2D { x, y, z: 0.5, w, r: rgba[0], g: rgba[1], b: rgba[2], a: rgba[3], u, v };
    [v(x0, y0, 0.0, 0.0), v(x1, y0, 1.0, 0.0), v(x1, y1, 1.0, 1.0), v(x0, y1, 0.0, 1.0)]
}

/// A tessellated square in the xy plane facing +z (normals +z), 4x4 quads.
fn grid() -> (Vec<MeshVertex>, Vec<u16>) {
    let mut v = Vec::new();
    for j in 0..5 {
        for i in 0..5 {
            let (x, y) = (i as f32 / 4.0 * 2.0 - 1.0, j as f32 / 4.0 * 2.0 - 1.0);
            v.push(MeshVertex { pos: [x, y, 0.0], normal: [0.0, 0.0, 1.0], color: [40, 40, 40, 255], uv: [i as f32 / 4.0, j as f32 / 4.0], uv2: [0.0; 2] });
        }
    }
    let mut ix = Vec::new();
    for j in 0..4u16 {
        for i in 0..4u16 {
            let a = j * 5 + i;
            ix.extend([a, a + 1, a + 6, a, a + 6, a + 5]);
        }
    }
    (v, ix)
}

fn texture(r: &mut Renderer, gpu: &Gpu) -> u32 {
    let mut px = Vec::new();
    for y in 0..8 {
        for x in 0..8 {
            let on = (x + y) % 2 == 0;
            px.extend(if on { [255, 220, 60, 255] } else { [30, 90, 200, 128] });
        }
    }
    r.upload_texture(gpu, 0, 8, 8, TexData::Rgba8, &[&px], true)
}

/// Perspective (looking down -z from z = 3), column-major, depth 0..1.
fn proj() -> Mat4 {
    let (n, f) = (0.5f32, 20.0f32);
    let t = 1.0 / (0.6f32).tan();
    [t, 0.0, 0.0, 0.0, 0.0, t, 0.0, 0.0, 0.0, 0.0, f / (n - f), -1.0, 0.0, 0.0, n * f / (n - f), 0.0]
}

fn view() -> Mat4 {
    [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, -3.0, 1.0]
}

fn record(r: &mut Renderer, gpu: &Gpu) {
    let tex = texture(r, gpu);
    // A render target texture (a camera raster) and a frame-copy texture.
    let blank = vec![0u8; 16 * 16 * 4];
    let rt = r.upload_texture(gpu, 0, 16, 16, TexData::Rgba8, &[&blank], false);
    let copy = r.upload_texture(gpu, 0, SIZE, SIZE, TexData::Rgba8, &[&vec![0u8; (SIZE * SIZE * 4) as usize]], false);
    let (gv, gi) = grid();
    let geom = r.upload_geometry(gpu, 0, &gv, &gi);
    let skin_v: Vec<SkinVertex> = gv.iter().map(|v| SkinVertex { base: *v, weights: [0.5, 0.5, 0.0, 0.0], indices: [0, 1, 0, 0] }).collect();
    let skin = r.upload_skinned_geometry(gpu, 0, &skin_v, &gi);

    // Into the render target: a clear and a red quad.
    r.set_target(rt);
    r.clear(Some([0.0, 0.3, 0.0, 1.0]), true);
    let st = DrawState { xform: [2.0 / 16.0, -2.0 / 16.0, -1.0, 1.0], z_test: 0, z_write: 0, alpha_func: 0, ..base() };
    r.draw_2d(&st, 4, &quad(4.0, 4.0, 12.0, 12.0, 1.0, [255, 0, 0, 255]), Some(&[0, 1, 2, 0, 2, 3]));
    r.set_target(0);

    r.clear(Some([0.1, 0.1, 0.2, 1.0]), true);
    let xf = Xform { proj: proj(), view: view(), world: IDENTITY };
    // Lit mesh: ambient, a directional light and a point light; textured.
    let mut model = ModelState::new(IDENTITY);
    model.ambient = [0.2, 0.2, 0.2, 1.0];
    model.surf_props = [1.0, 0.0, 1.0, 0.0];
    model.lights[0] = [1.0, 0.0, 0.0, 0.0];
    model.lights[2] = [0.3, -0.3, -1.0, 0.0];
    model.lights[3] = [0.6, 0.5, 0.4, 1.0];
    model.lights[4] = [2.0, 3.0, 0.0, 0.0];
    model.lights[5] = [0.5, 0.5, 0.5, 1.0];
    model.lights[7] = [0.0, 0.8, 1.0, 1.0];
    let mesh_state = DrawState { texture: tex, ..base() };
    r.draw_mesh(&mesh_state, geom, 0, gi.len() as u32, &xf, &model, &MeshExtras::default());
    // Skinned copy, moved by two bones.
    let mut b0 = IDENTITY;
    b0[12] = -0.5;
    let mut b1 = IDENTITY;
    b1[13] = 0.4;
    let mut small = model;
    small.world = [0.4, 0.0, 0.0, 0.0, 0.0, 0.4, 0.0, 0.0, 0.0, 0.0, 0.4, 0.0, 0.5, -0.5, 0.2, 1.0];
    r.draw_mesh(&DrawState { texture: rt, ..base() }, skin, 0, gi.len() as u32, &xf, &small, &MeshExtras { bones: Some(&[b0, b1]), env: None });
    // Environment-mapped mesh (MatFX).
    let mut env = small;
    env.world[12] = -0.5;
    env.world[13] = 0.5;
    env.tex_matrix = [0.5, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.5, 0.5, 0.0, 1.0];
    env.env_color = [1.0, 1.0, 1.0, 1.0];
    env.fx_params = [0.6, 0.0, 0.0, 0.0];
    r.draw_mesh(&DrawState { texture: tex, src_blend: 2, ..base() }, geom, 0, gi.len() as u32, &xf, &env, &MeshExtras { bones: None, env: Some(EnvMap { texture: tex, filter: 2, address_u: 1, address_v: 1 }) });
    // Im3D line strip and triangles, fogged.
    let v3 = |x: f32, y: f32, z: f32, c: [u8; 4]| Vertex3D { x, y, z, r: c[0], g: c[1], b: c[2], a: c[3], u: 0.0, v: 0.0 };
    r.draw_3d(&DrawState { fog: 1, ..base() }, 3, &[v3(-1.0, -1.0, -4.0, [255, 255, 255, 255]), v3(1.0, -1.0, -4.0, [255, 0, 255, 255]), v3(0.0, 0.2, -6.0, [0, 255, 255, 255])], None, &xf);
    r.draw_3d(&DrawState { z_test: 0, ..base() }, 2, &[v3(-1.0, 0.8, 0.0, [255, 255, 0, 255]), v3(1.0, 0.9, 0.0, [255, 255, 0, 255])], None, &xf);
    // Copy of the frame so far, drawn back small (a post-effect pattern).
    r.copy_frame(copy, 0, 0);
    r.begin_overlay();
    // HUD: textured, alpha-blended, alpha-tested quads; one from the frame copy.
    r.draw_2d(&DrawState { texture: tex, z_test: 0, z_write: 0, blend: 1, ..base() }, 4, &quad(2.0, 40.0, 22.0, 60.0, 1.0, [255, 255, 255, 200]), Some(&[0, 1, 2, 0, 2, 3]));
    r.draw_2d(&DrawState { texture: copy, z_test: 0, z_write: 0, ..base() }, 4, &quad(44.0, 2.0, 62.0, 20.0, 1.0, [255, 255, 255, 255]), Some(&[0, 1, 2, 0, 2, 3]));
    // Screen droplets style: two UV sets.
    let d = |x: f32, y: f32, u: f32, v: f32| Vertex2DUV2 { base: Vertex2D { x, y, z: 0.0, w: 1.0, r: 255, g: 255, b: 255, a: 255, u, v }, u2: u * 0.5, v2: v * 0.5 };
    r.draw_2d_uv2(&DrawState { texture: tex, z_test: 0, z_write: 0, ..base() }, 4, &[d(26.0, 44.0, 0.0, 0.0), d(40.0, 44.0, 1.0, 0.0), d(40.0, 58.0, 1.0, 1.0), d(26.0, 58.0, 0.0, 1.0)], Some(&[0, 1, 2, 0, 2, 3]), &EnvMap { texture: rt, filter: 2, address_u: 3, address_v: 3 });
    // PS2/Xbox colour filter over the copy (CPostFX).
    let cf = DrawState { texture: copy, z_test: 0, z_write: 0, shader: 1, shader_params: [0.2, 0.15, 0.1, 0.3, 0.0, 0.0, 0.0, 0.0], ..base() };
    r.draw_2d(&cf, 4, &quad(44.0, 40.0, 62.0, 60.0, 1.0, [255, 255, 255, 255]), Some(&[0, 1, 2, 0, 2, 3]));
}

fn run(backend: Backend) {
    run_with(backend, 0, 100, true);
}

/// Every antialiasing level, and a render scale, with the HUD marker (the overlay split).
fn levels(backend: Backend) {
    let mut worst = Vec::new();
    for level in 0..Antialiasing::LEVELS.len() as u32 {
        worst.push((Antialiasing::LEVELS[level as usize].0, 100, run_with(backend, level, 100, false)));
    }
    worst.push(("Off", 75, run_with(backend, 0, 75, false)));
    worst.push(("SSAA 2x", 75, run_with(backend, 7, 75, false)));
    for (name, scale, w) in &worst {
        eprintln!("{backend:?} {name} at {scale} %: {:.3} % within 2", w * 100.0);
    }
    for (name, scale, w) in worst {
        assert!(w >= GATE, "{backend:?} {name} at {scale} %: only {:.3} % within 2 levels", w * 100.0);
    }
}

fn run_with(backend: Backend, level: u32, render_scale: u32, strict: bool) -> f64 {
    let gpu = match Gpu::headless(backend) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("SKIPPED on {backend:?}: {e}");
            return 1.0;
        }
    };
    let mut r = Renderer::new(&gpu, wgpu::TextureFormat::Rgba8Unorm);
    r.set_antialiasing(&gpu, Antialiasing::level(level));
    r.set_render_scale(render_scale);
    let mut worst = 1.0f64;
    let mut shadow = Shadow::new(&gpu);
    let out = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    // Two frames: the second redraws with resources from the first (retained geometry/textures).
    for n in 0..2 {
        record(&mut r, &gpu);
        let mut frame = r.front.finish();
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        r.back.encode(&gpu, &mut enc, &out.create_view(&Default::default()), (SIZE, SIZE), &mut frame);
        gpu.queue.submit([enc.finish()]);
        r.back.end_frame(&frame);
        let c = shadow.run(&gpu, &r.back, &frame, (SIZE, SIZE), true).expect("comparison");
        r.front.recycle(frame);
        eprintln!("{backend:?} frame {n}: {:.3} % within 2, max diff {}", c.within * 100.0, c.max_diff);
        if let Some((a, b)) = &c.images {
            let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
            for (name, img) in [("wgpu", a), ("retro", b)] {
                let f = std::fs::File::create(dir.join(format!("shadow-{backend:?}-l{level}-s{render_scale}-{n}-{name}.png"))).unwrap();
                let mut e = png::Encoder::new(f, c.width, c.height);
                e.set_color(png::ColorType::Rgba);
                e.write_header().unwrap().write_image_data(img).unwrap();
            }
            eprintln!("images in {}", dir.display());
        }
        worst = worst.min(c.within);
        if !strict {
            continue;
        }
        assert!(c.within >= GATE, "{backend:?} frame {n}: only {:.3} % of pixels within 2 levels (max diff {})", c.within * 100.0, c.max_diff);
        // The image isn't trivially empty: the drawn parts have many colours.
        let img = shot(&gpu, &r);
        let colours: std::collections::HashSet<[u8; 4]> = img.as_chunks::<4>().0.iter().copied().collect();
        assert!(colours.len() > 40, "{backend:?}: only {} colours in the frame", colours.len());
        if n == 1 {
            let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
            let f = std::fs::File::create(dir.join(format!("shadow-{backend:?}.png"))).unwrap();
            let mut e = png::Encoder::new(f, SIZE, SIZE);
            e.set_color(png::ColorType::Rgba);
            e.write_header().unwrap().write_image_data(&img).unwrap();
        }
        let _ = compare;
    }
    worst
}

/// The wgpu back end's last image (before the output stage).
fn shot(gpu: &Gpu, r: &Renderer) -> Vec<u8> {
    let t = r.back.scene_texture().unwrap();
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    let rb = revc_rw::frame::readback_start(gpu, &mut enc, t, (t.width(), t.height()));
    gpu.queue.submit([enc.finish()]);
    revc_rw::frame::readback_finish(gpu, rb).unwrap()
}

#[test]
fn shadow_vulkan() {
    run(Backend::Vulkan);
}

#[test]
fn shadow_gl() {
    run(Backend::Gl);
}

#[test]
fn shadow_levels_vulkan() {
    levels(Backend::Vulkan);
}

#[test]
fn shadow_levels_gl() {
    levels(Backend::Gl);
}
