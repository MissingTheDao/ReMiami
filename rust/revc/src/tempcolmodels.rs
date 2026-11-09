//! CTempColModels::Initialise (C++ src/collision/TempColModels.cpp): the built-in collision models
//! for peds, car parts, body parts, cutscene objects and weapons. The models stay C++ statics; this
//! fills them and owns their sphere arrays. Checked against the original by
//! tests/golden/tempcolmodels.txt. (Translated by the local model, reviewed.)
#![allow(non_upper_case_globals, dead_code, static_mut_refs, unsafe_op_in_unsafe_fn)]

use crate::collision::{ColModel, ColSphere, Sphere, BBox};
use crate::math::V3;

const LEVEL_GENERIC: u8 = 0;
const SURFACE_DEFAULT: u8 = 0;
const SURFACE_TARMAC: u8 = 1;
const SURFACE_GRASS: u8 = 2;
const SURFACE_GRAVEL: u8 = 3;
const SURFACE_MUD_DRY: u8 = 4;
const SURFACE_PAVEMENT: u8 = 5;
const SURFACE_CAR: u8 = 6;
const SURFACE_GLASS: u8 = 7;
const SURFACE_TRANSPARENT_CLOTH: u8 = 8;
const SURFACE_GARAGE_DOOR: u8 = 9;
const SURFACE_CAR_PANEL: u8 = 10;
const SURFACE_THICK_METAL_PLATE: u8 = 11;
const SURFACE_SCAFFOLD_POLE: u8 = 12;
const SURFACE_LAMP_POST: u8 = 13;
const SURFACE_FIRE_HYDRANT: u8 = 14;
const SURFACE_GIRDER: u8 = 15;
const SURFACE_METAL_CHAIN_FENCE: u8 = 16;
const SURFACE_PED: u8 = 17;
const SURFACE_SAND: u8 = 18;
const SURFACE_WATER: u8 = 19;
const SURFACE_WOOD_CRATES: u8 = 20;
const SURFACE_WOOD_BENCH: u8 = 21;
const SURFACE_WOOD_SOLID: u8 = 22;
const SURFACE_RUBBER: u8 = 23;
const SURFACE_PLASTIC: u8 = 24;
const SURFACE_HEDGE: u8 = 25;
const SURFACE_STEEP_CLIFF: u8 = 26;
const SURFACE_CONTAINER: u8 = 27;
const SURFACE_NEWS_VENDOR: u8 = 28;
const SURFACE_WHEELBASE: u8 = 29;
const SURFACE_CARDBOARDBOX: u8 = 30;
const SURFACE_TRANSPARENT_STONE: u8 = 31;

static mut s_aPedSpheres: [ColSphere; 3] = [
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
];
static mut s_aPed2Spheres: [ColSphere; 3] = [
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
];
static mut s_aPedGSpheres: [ColSphere; 4] = [
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
];
static mut s_aDoorSpheres: [ColSphere; 3] = [
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
];
static mut s_aBumperSpheres: [ColSphere; 4] = [
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
];
static mut s_aPanelSpheres: [ColSphere; 4] = [
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
];
static mut s_aBonnetSpheres: [ColSphere; 4] = [
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
];
static mut s_aBootSpheres: [ColSphere; 4] = [
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
];
static mut s_aWheelSpheres: [ColSphere; 2] = [
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
];
static mut s_aBodyPartSpheres1: [ColSphere; 2] = [
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
];
static mut s_aBodyPartSpheres2: [ColSphere; 2] = [
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
    ColSphere { center: V3::new(0.0, 0.0, 0.0), radius: 0.0, surface: 0, piece: 0 },
];

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_temp_col_models_init(models: *const *mut ColModel) {
    unsafe { initialise(&*(models as *const [*mut ColModel; 18])) }
}

pub unsafe fn initialise(models: &[*mut ColModel; 18]) {
    let models = *models;
    // Order: Ped1, Ped2, BBox, Bumper1, Wheel1, Panel1, BodyPart2, BodyPart1, CutObj0..CutObj4, PedGroundHit, Boot1, Door1, Bonnet1, Weapon
    // Indices based on C++ member order:
    // 0: Ped1, 1: Ped2, 2: BBox, 3: Bumper1, 4: Wheel1, 5: Panel1, 6: BodyPart2, 7: BodyPart1
    // 8..12: CutObj0..4, 13: PedGroundHit, 14: Boot1, 15: Door1, 16: Bonnet1, 17: Weapon

    // BBox
    let m = &mut *models[2];
    m.bounding_sphere = Sphere::new(2.0, V3::new(0.0, 0.0, 0.0));
    m.bounding_box = BBox::new(V3::new(-2.0, -2.0, -2.0), V3::new(2.0, 2.0, 2.0));
    m.level = LEVEL_GENERIC;

    // CutObj
    for i in 0..5 {
        let m = &mut *models[8 + i];
        m.bounding_sphere = Sphere::new(2.0, V3::new(0.0, 0.0, 0.0));
        m.bounding_box = BBox::new(V3::new(-2.0, -2.0, -2.0), V3::new(2.0, 2.0, 2.0));
        m.level = LEVEL_GENERIC;
    }

    // Ped Spheres
    for i in 0..3 {
        let s = &mut *s_aPedSpheres.get_unchecked_mut(i);
        s.radius = 0.35;
    }
    (*s_aPedSpheres.get_unchecked_mut(0)).center = V3::new(0.0, 0.0, -0.25);
    (*s_aPedSpheres.get_unchecked_mut(1)).center = V3::new(0.0, 0.0, 0.15);
    (*s_aPedSpheres.get_unchecked_mut(2)).center = V3::new(0.0, 0.0, 0.55);

    for i in 0..3 {
        let s = &mut *s_aPedSpheres.get_unchecked_mut(i);
        s.surface = SURFACE_PED;
        s.piece = 0;
    }

    // Ped1
    let m = &mut *models[0];
    m.bounding_sphere = Sphere::new(1.25, V3::new(0.0, 0.0, 0.0));
    m.bounding_box = BBox::new(V3::new(-0.35, -0.35, -1.0), V3::new(0.35, 0.35, 0.9));
    m.num_spheres = 3;
    m.spheres = s_aPedSpheres.as_mut_ptr();
    m.level = LEVEL_GENERIC;
    m.owns_collision_volumes = false;

    // Ped2 Spheres
    (*s_aPed2Spheres.get_unchecked_mut(0)).radius = 0.3;
    (*s_aPed2Spheres.get_unchecked_mut(1)).radius = 0.4;
    (*s_aPed2Spheres.get_unchecked_mut(2)).radius = 0.3;

    (*s_aPed2Spheres.get_unchecked_mut(0)).center = V3::new(0.0, 0.35, -0.9);
    (*s_aPed2Spheres.get_unchecked_mut(1)).center = V3::new(0.0, 0.0, -0.9);
    (*s_aPed2Spheres.get_unchecked_mut(2)).center = V3::new(0.0, -0.35, -0.9);

    for i in 0..3 {
        let s = &mut *s_aPed2Spheres.get_unchecked_mut(i);
        s.surface = SURFACE_PED;
        s.piece = 0;
    }

    // Ped2
    let m = &mut *models[1];
    m.bounding_sphere = Sphere::new(2.0, V3::new(0.0, 0.0, 0.0));
    m.bounding_box = BBox::new(V3::new(-0.7, -0.7, -1.2), V3::new(0.7, 0.7, 0.0));
    m.num_spheres = 3;
    m.spheres = s_aPed2Spheres.as_mut_ptr();
    m.level = LEVEL_GENERIC;
    m.owns_collision_volumes = false;

    // Ped Ground Hit Spheres
    (*s_aPedGSpheres.get_unchecked_mut(0)).radius = 0.35;
    (*s_aPedGSpheres.get_unchecked_mut(1)).radius = 0.35;
    (*s_aPedGSpheres.get_unchecked_mut(2)).radius = 0.35;
    (*s_aPedGSpheres.get_unchecked_mut(3)).radius = 0.3;

    (*s_aPedGSpheres.get_unchecked_mut(0)).center = V3::new(0.0, -0.4, -0.9);
    (*s_aPedGSpheres.get_unchecked_mut(1)).center = V3::new(0.0, -0.1, -0.9);
    (*s_aPedGSpheres.get_unchecked_mut(2)).center = V3::new(0.0, 0.25, -0.9);
    (*s_aPedGSpheres.get_unchecked_mut(3)).center = V3::new(0.0, 0.65, -0.9);

    (*s_aPedGSpheres.get_unchecked_mut(0)).surface = SURFACE_PED;
    (*s_aPedGSpheres.get_unchecked_mut(1)).surface = SURFACE_PED;
    (*s_aPedGSpheres.get_unchecked_mut(2)).surface = SURFACE_PED;
    (*s_aPedGSpheres.get_unchecked_mut(3)).surface = SURFACE_PED;
    (*s_aPedGSpheres.get_unchecked_mut(0)).piece = 4;
    (*s_aPedGSpheres.get_unchecked_mut(1)).piece = 1;
    (*s_aPedGSpheres.get_unchecked_mut(2)).piece = 0;
    (*s_aPedGSpheres.get_unchecked_mut(3)).piece = 6;

    // PedGroundHit
    let m = &mut *models[13];
    m.bounding_sphere = Sphere::new(2.0, V3::new(0.0, 0.0, 0.0));
    m.bounding_box = BBox::new(V3::new(-0.4, -1.0, -1.25), V3::new(0.4, 1.2, -0.5));
    m.num_spheres = 4;
    m.spheres = s_aPedGSpheres.as_mut_ptr();
    m.level = LEVEL_GENERIC;
    m.owns_collision_volumes = false;

    // Door Spheres
    (*s_aDoorSpheres.get_unchecked_mut(0)).radius = 0.15;
    (*s_aDoorSpheres.get_unchecked_mut(1)).radius = 0.15;
    (*s_aDoorSpheres.get_unchecked_mut(2)).radius = 0.25;

    (*s_aDoorSpheres.get_unchecked_mut(0)).center = V3::new(0.0, -0.25, -0.35);
    (*s_aDoorSpheres.get_unchecked_mut(1)).center = V3::new(0.0, -0.95, -0.35);
    (*s_aDoorSpheres.get_unchecked_mut(2)).center = V3::new(0.0, -0.6, 0.25);

    for i in 0..3 {
        let s = &mut *s_aDoorSpheres.get_unchecked_mut(i);
        s.surface = SURFACE_CAR_PANEL;
        s.piece = 0;
    }

    // Door1
    let m = &mut *models[15];
    m.bounding_sphere = Sphere::new(1.5, V3::new(0.0, -0.6, 0.0));
    m.bounding_box = BBox::new(V3::new(-0.3, 0.0, -0.6), V3::new(0.3, -1.2, 0.6));
    m.num_spheres = 3;
    m.spheres = s_aDoorSpheres.as_mut_ptr();
    m.level = LEVEL_GENERIC;
    m.owns_collision_volumes = false;

    // Bumper Spheres
    for i in 0..4 {
        let s = &mut *s_aBumperSpheres.get_unchecked_mut(i);
        s.radius = 0.15;
    }
    (*s_aBumperSpheres.get_unchecked_mut(0)).center = V3::new(0.85, -0.05, 0.0);
    (*s_aBumperSpheres.get_unchecked_mut(1)).center = V3::new(0.4, 0.05, 0.0);
    (*s_aBumperSpheres.get_unchecked_mut(2)).center = V3::new(-0.4, 0.05, 0.0);
    (*s_aBumperSpheres.get_unchecked_mut(3)).center = V3::new(-0.85, -0.05, 0.0);

    for i in 0..4 {
        let s = &mut *s_aBumperSpheres.get_unchecked_mut(i);
        s.surface = SURFACE_CAR_PANEL;
        s.piece = 0;
    }

    // Bumper1
    let m = &mut *models[3];
    m.bounding_sphere = Sphere::new(2.2, V3::new(0.0, -0.6, 0.0));
    m.bounding_box = BBox::new(V3::new(-1.2, -0.3, -0.2), V3::new(1.2, 0.3, 0.2));
    m.num_spheres = 4;
    m.spheres = s_aBumperSpheres.as_mut_ptr();
    m.level = LEVEL_GENERIC;
    m.owns_collision_volumes = false;

    // Panel Spheres
    for i in 0..4 {
        let s = &mut *s_aPanelSpheres.get_unchecked_mut(i);
        s.radius = 0.15;
    }
    (*s_aPanelSpheres.get_unchecked_mut(0)).center = V3::new(0.15, 0.45, 0.0);
    (*s_aPanelSpheres.get_unchecked_mut(1)).center = V3::new(0.15, -0.45, 0.0);
    (*s_aPanelSpheres.get_unchecked_mut(2)).center = V3::new(-0.15, -0.45, 0.0);
    (*s_aPanelSpheres.get_unchecked_mut(3)).center = V3::new(-0.15, 0.45, 0.0);

    for i in 0..4 {
        let s = &mut *s_aPanelSpheres.get_unchecked_mut(i);
        s.surface = SURFACE_CAR_PANEL;
        s.piece = 0;
    }

    // Panel1
    let m = &mut *models[5];
    m.bounding_sphere = Sphere::new(1.4, V3::new(0.0, 0.0, 0.0));
    m.bounding_box = BBox::new(V3::new(-0.3, -0.6, -0.15), V3::new(0.3, 0.6, 0.15));
    m.num_spheres = 4;
    m.spheres = s_aPanelSpheres.as_mut_ptr();
    m.level = LEVEL_GENERIC;
    m.owns_collision_volumes = false;

    // Bonnet Spheres
    for i in 0..4 {
        let s = &mut *s_aBonnetSpheres.get_unchecked_mut(i);
        s.radius = 0.2;
    }
    (*s_aBonnetSpheres.get_unchecked_mut(0)).center = V3::new(-0.4, 0.1, 0.0);
    (*s_aBonnetSpheres.get_unchecked_mut(1)).center = V3::new(-0.4, 0.9, 0.0);
    (*s_aBonnetSpheres.get_unchecked_mut(2)).center = V3::new(0.4, 0.1, 0.0);
    (*s_aBonnetSpheres.get_unchecked_mut(3)).center = V3::new(0.4, 0.9, 0.0);

    for i in 0..4 {
        let s = &mut *s_aBonnetSpheres.get_unchecked_mut(i);
        s.surface = SURFACE_CAR_PANEL;
        s.piece = 0;
    }

    // Bonnet1
    let m = &mut *models[16];
    m.bounding_sphere = Sphere::new(1.7, V3::new(0.0, 0.5, 0.0));
    m.bounding_box = BBox::new(V3::new(-0.7, -0.2, -0.3), V3::new(0.7, 1.2, 0.3));
    m.num_spheres = 4;
    m.spheres = s_aBonnetSpheres.as_mut_ptr();
    m.level = LEVEL_GENERIC;
    m.owns_collision_volumes = false;

    // Boot Spheres
    for i in 0..4 {
        let s = &mut *s_aBootSpheres.get_unchecked_mut(i);
        s.radius = 0.2;
    }
    (*s_aBootSpheres.get_unchecked_mut(0)).center = V3::new(-0.4, -0.1, 0.0);
    (*s_aBootSpheres.get_unchecked_mut(1)).center = V3::new(-0.4, -0.6, 0.0);
    (*s_aBootSpheres.get_unchecked_mut(2)).center = V3::new(0.4, -0.1, 0.0);
    (*s_aBootSpheres.get_unchecked_mut(3)).center = V3::new(0.4, -0.6, 0.0);

    for i in 0..4 {
        let s = &mut *s_aBootSpheres.get_unchecked_mut(i);
        s.surface = SURFACE_CAR_PANEL;
        s.piece = 0;
    }

    // Boot1
    let m = &mut *models[14];
    m.bounding_sphere = Sphere::new(1.4, V3::new(0.0, -0.4, 0.0));
    m.bounding_box = BBox::new(V3::new(-0.7, -0.9, -0.3), V3::new(0.7, 0.2, 0.3));
    m.num_spheres = 4;
    m.spheres = s_aBootSpheres.as_mut_ptr();
    m.level = LEVEL_GENERIC;
    m.owns_collision_volumes = false;

    // Wheel Spheres
    (*s_aWheelSpheres.get_unchecked_mut(0)).radius = 0.35;
    (*s_aWheelSpheres.get_unchecked_mut(1)).radius = 0.35;

    (*s_aWheelSpheres.get_unchecked_mut(0)).center = V3::new(-0.3, 0.0, 0.0);
    (*s_aWheelSpheres.get_unchecked_mut(1)).center = V3::new(0.3, 0.0, 0.0);

    for i in 0..2 {
        let s = &mut *s_aWheelSpheres.get_unchecked_mut(i);
        s.surface = SURFACE_WHEELBASE;
        s.piece = 0;
    }

    // Wheel1
    let m = &mut *models[4];
    m.bounding_sphere = Sphere::new(1.4, V3::new(0.0, 0.0, 0.0));
    m.bounding_box = BBox::new(V3::new(-0.7, -0.4, -0.4), V3::new(0.7, 0.4, 0.4));
    m.num_spheres = 2;
    m.spheres = s_aWheelSpheres.as_mut_ptr();
    m.level = LEVEL_GENERIC;
    m.owns_collision_volumes = false;

    // Body Part Spheres 1
    (*s_aBodyPartSpheres1.get_unchecked_mut(0)).radius = 0.2;
    (*s_aBodyPartSpheres1.get_unchecked_mut(1)).radius = 0.2;

    (*s_aBodyPartSpheres1.get_unchecked_mut(0)).center = V3::new(0.0, 0.0, 0.0);
    (*s_aBodyPartSpheres1.get_unchecked_mut(1)).center = V3::new(0.8, 0.0, 0.0);

    for i in 0..2 {
        let s = &mut *s_aBodyPartSpheres1.get_unchecked_mut(i);
        s.surface = SURFACE_PED;
        s.piece = 0;
    }

    // BodyPart1
    let m = &mut *models[7];
    m.bounding_sphere = Sphere::new(0.7, V3::new(0.4, 0.0, 0.0));
    m.bounding_box = BBox::new(V3::new(-0.3, -0.3, -0.3), V3::new(1.1, 0.3, 0.3));
    m.num_spheres = 2;
    m.spheres = s_aBodyPartSpheres1.as_mut_ptr();
    m.level = LEVEL_GENERIC;
    m.owns_collision_volumes = false;

    // Body Part Spheres 2
    (*s_aBodyPartSpheres2.get_unchecked_mut(0)).radius = 0.15;
    (*s_aBodyPartSpheres2.get_unchecked_mut(1)).radius = 0.15;

    (*s_aBodyPartSpheres2.get_unchecked_mut(0)).center = V3::new(0.0, 0.0, 0.0);
    (*s_aBodyPartSpheres2.get_unchecked_mut(1)).center = V3::new(0.5, 0.0, 0.0);

    for i in 0..2 {
        let s = &mut *s_aBodyPartSpheres2.get_unchecked_mut(i);
        s.surface = SURFACE_PED;
        s.piece = 0;
    }

    // BodyPart2
    let m = &mut *models[6];
    m.bounding_sphere = Sphere::new(0.5, V3::new(0.25, 0.0, 0.0));
    m.bounding_box = BBox::new(V3::new(-0.2, -0.2, -0.2), V3::new(0.7, 0.2, 0.2));
    m.num_spheres = 2;
    m.spheres = s_aBodyPartSpheres2.as_mut_ptr();
    m.level = LEVEL_GENERIC;
    m.owns_collision_volumes = false;

    // Weapon
    let m = &mut *models[17];
    m.bounding_sphere.radius = 0.25;
    m.bounding_box.min = V3::new(-0.25, -0.25, -0.25);
    m.bounding_box.max = V3::new(0.25, 0.25, 0.25);
}

#[cfg(test)]
mod golden {
    use super::*;
    use crate::collision::Sphere;

    const NAMES: [&str; 18] = [
        "Ped1", "Ped2", "BBox", "Bumper1", "Wheel1", "Panel1", "BodyPart2", "BodyPart1", "CutObj0", "CutObj1", "CutObj2", "CutObj3",
        "CutObj4", "PedGroundHit", "Boot1", "Door1", "Bonnet1", "Weapon",
    ];

    fn empty() -> ColModel {
        ColModel {
            bounding_sphere: Sphere::default(),
            bounding_box: BBox::default(),
            num_spheres: 0,
            num_boxes: 0,
            num_triangles: 0,
            num_lines: 0,
            level: 0,
            owns_collision_volumes: false,
            spheres: std::ptr::null_mut(),
            lines: std::ptr::null_mut(),
            boxes: std::ptr::null_mut(),
            vertices: std::ptr::null_mut(),
            triangles: std::ptr::null_mut(),
            triangle_planes: std::ptr::null_mut(),
        }
    }

    /// The models as the original leaves them (tests/golden/recorders/tempcolmodels).
    #[test]
    fn matches_cpp_golden() {
        let mut models: Vec<ColModel> = (0..18).map(|_| empty()).collect();
        let ptrs: [*mut ColModel; 18] = std::array::from_fn(|i| &mut models[i] as *mut ColModel);
        unsafe { initialise(&ptrs) };
        let v = |v: V3| format!(" {:08x} {:08x} {:08x}", v.x.to_bits(), v.y.to_bits(), v.z.to_bits());
        let mut out = String::new();
        for (m, name) in models.iter().zip(NAMES) {
            out += &format!("{name}{} {:08x}{}{}", v(m.bounding_sphere.center), m.bounding_sphere.radius.to_bits(), v(m.bounding_box.min), v(m.bounding_box.max));
            out += &format!(" {} {} {} {} {} {}", m.num_spheres, m.num_boxes, m.num_triangles, m.num_lines, m.level, m.owns_collision_volumes as i32);
            for i in 0..m.num_spheres as usize {
                let s = unsafe { *m.spheres.add(i) };
                out += &format!(" |{} {:08x} {} {}", v(s.center), s.radius.to_bits(), s.surface, s.piece);
            }
            out += "\n";
        }
        let want = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/tempcolmodels.txt")).unwrap();
        for (a, b) in out.lines().zip(want.lines()) {
            assert_eq!(a, b);
        }
        assert_eq!(out.lines().count(), want.lines().count());
    }
}
