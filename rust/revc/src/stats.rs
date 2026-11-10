//! Player statistics (C++ src/core/Stats.cpp, CStats). Rust owns the storage: `revc_stats` below,
//! which the C++ statics are references into (so the inline reads and increments all over the
//! game still work). Initialisation, the rating, progress and record keeping, and the save block
//! are here; the stats screen text stays C++. Identical to the original (tests/golden/stats.txt).
#![allow(non_snake_case, non_upper_case_globals)]

use std::ffi::{CStr, c_char};

pub const NUM_RADIOS: usize = 10;
pub const TOTAL_FASTEST_TIMES: usize = 23;
pub const TOTAL_HIGHEST_SCORES: usize = 5;
pub const TOTAL_BEST_POSITIONS: usize = 1;
pub const TOTAL_PROPERTIES: usize = 15;

/// All CStats statics, in Stats.cpp's definition order (the C++ mirror RevcStats matches).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stats {
    pub SeagullsKilled: i32,
    pub BoatsExploded: i32,
    pub WantedStarsAttained: i32,
    pub WantedStarsEvaded: i32,
    pub DaysPassed: i32,
    pub HeadsPopped: i32,
    pub CommercialPassed: i32,
    pub IndustrialPassed: i32,
    pub SuburbanPassed: i32,
    pub NumberKillFrenziesPassed: i32,
    pub PeopleKilledByOthers: i32,
    pub HelisDestroyed: i32,
    pub PedsKilledOfThisType: [i32; 23],
    pub TimesDied: i32,
    pub TimesArrested: i32,
    pub KillsSinceLastCheckpoint: i32,
    pub DistanceTravelledByCar: f32,
    pub DistanceTravelledByHelicoptor: f32,
    pub DistanceTravelledByBike: f32,
    pub DistanceTravelledByBoat: f32,
    pub DistanceTravelledByPlane: f32,
    pub DistanceTravelledByGolfCart: f32,
    pub DistanceTravelledOnFoot: f32,
    pub FlightTime: i32,
    pub TimesDrowned: i32,
    pub PhotosTaken: i32,
    pub LoanSharks: f32,
    pub StoresKnockedOff: f32,
    pub MovieStunts: f32,
    pub Assassinations: f32,
    pub PizzasDelivered: f32,
    pub GarbagePickups: f32,
    pub IceCreamSold: f32,
    pub TopShootingRangeScore: f32,
    pub ShootingRank: f32,
    pub ProgressMade: f32,
    pub TotalProgressInGame: f32,
    pub CarsExploded: i32,
    pub PeopleKilledByPlayer: i32,
    pub MaximumJumpDistance: f32,
    pub MaximumJumpHeight: f32,
    pub MaximumJumpFlips: i32,
    pub MaximumJumpSpins: i32,
    pub BestStuntJump: i32,
    pub NumberOfUniqueJumpsFound: i32,
    pub TotalNumberOfUniqueJumps: i32,
    pub PassengersDroppedOffWithTaxi: i32,
    pub MoneyMadeWithTaxi: i32,
    pub MissionsGiven: i32,
    pub MissionsPassed: i32,
    pub LastMissionPassedName: [u8; 8],
    pub TotalLegitimateKills: i32,
    pub LivesSavedWithAmbulance: i32,
    pub CriminalsCaught: i32,
    pub HighestLevelAmbulanceMission: i32,
    pub HighestLevelVigilanteMission: i32,
    pub HighestLevelFireMission: i32,
    pub FiresExtinguished: i32,
    pub TotalNumberKillFrenzies: i32,
    pub TotalNumberMissions: i32,
    pub RoundsFiredByPlayer: i32,
    pub KgsOfExplosivesUsed: i32,
    pub BulletsThatHit: i32,
    pub BestTimeBombDefusal: i32,
    pub FastestTimes: [i32; TOTAL_FASTEST_TIMES],
    pub HighestScores: [i32; TOTAL_HIGHEST_SCORES],
    pub BestPositions: [i32; TOTAL_BEST_POSITIONS],
    pub PropertyOwned: [bool; TOTAL_PROPERTIES],
    pub NumPropertyOwned: i32,
    pub PropertyDestroyed: i32,
    pub HighestChaseValue: f32,
    pub CheatedCount: i32,
    pub ShowChaseStatOnScreen: i32,
    pub PamphletMissionPassed: i32,
    pub abSonyCDs: [bool; 1],
    pub BloodRingKills: i32,
    pub BloodRingTime: i32,
    pub FavoriteRadioStationList: [f32; NUM_RADIOS],
    pub Sprayings: i32,
    pub AutoPaintingBudget: f32,
    pub NoMoreHurricanes: i32,
    pub FashionBudget: f32,
    pub PropertyBudget: f32,
    pub WeaponBudget: f32,
    pub SafeHouseVisits: i32,
    pub TyresPopped: i32,
    pub LongestWheelie: i32,
    pub LongestStoppie: i32,
    pub Longest2Wheel: i32,
    pub LongestWheelieDist: f32,
    pub LongestStoppieDist: f32,
    pub Longest2WheelDist: f32,
}

/// The game's stats; C++ binds its CStats references to the fields.
#[unsafe(no_mangle)]
pub static mut revc_stats: Stats = Stats::ZERO;

/// x86 cvttss2si: out of range or NaN gives i32::MIN (Rust `as` would saturate).
fn f2i(f: f32) -> i32 {
    if f.is_nan() || f >= 2147483648.0 || f < -2147483648.0 { i32::MIN } else { f as i32 }
}

struct Reader<'a> {
    b: &'a [u8],
}

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> [u8; N] {
        let (h, t) = self.b.split_at(N);
        self.b = t;
        h.try_into().unwrap()
    }
    fn i32(&mut self) -> i32 { i32::from_le_bytes(self.take()) }
    fn f32(&mut self) -> f32 { f32::from_le_bytes(self.take()) }
    fn u8(&mut self) -> u8 { self.take::<1>()[0] }
    fn bool(&mut self) -> bool { self.u8() != 0 }
}

/// Criminal rating thresholds: below each, that RATNG key.
const RATINGS: [(i32, &CStr); 48] = [
    (20, c"RATNG1"), (50, c"RATNG2"), (75, c"RATNG3"), (100, c"RATNG4"), (120, c"RATNG5"),
    (150, c"RATNG6"), (200, c"RATNG7"), (240, c"RATNG8"), (270, c"RATNG9"), (300, c"RATNG10"),
    (335, c"RATNG11"), (370, c"RATNG12"), (400, c"RATNG13"), (450, c"RATNG14"), (500, c"RATNG15"),
    (550, c"RATNG16"), (600, c"RATNG17"), (610, c"RATNG18"), (650, c"RATNG19"), (700, c"RATNG20"),
    (850, c"RATNG21"), (1000, c"RATNG22"), (1005, c"RATNG23"), (1150, c"RATNG24"),
    (1300, c"RATNG25"), (1500, c"RATNG26"), (1700, c"RATNG27"), (2000, c"RATNG28"),
    (2100, c"RATNG29"), (2300, c"RATNG30"), (2500, c"RATNG31"), (2750, c"RATNG32"),
    (3000, c"RATNG33"), (3500, c"RATNG34"), (4000, c"RATNG35"), (5000, c"RATNG36"),
    (7500, c"RATNG37"), (10000, c"RATNG38"), (20000, c"RATNG39"), (30000, c"RATNG40"),
    (40000, c"RATNG41"), (50000, c"RATNG42"), (65000, c"RATNG43"), (80000, c"RATNG44"),
    (100000, c"RATNG45"), (150000, c"RATNG46"), (200000, c"RATNG47"), (300000, c"RATNG48"),
];

const CHASES: [(f32, &CStr); 20] = [
    (20.0, c"CHASE1"), (50.0, c"CHASE2"), (75.0, c"CHASE3"), (100.0, c"CHASE4"), (150.0, c"CHASE5"),
    (200.0, c"CHASE6"), (250.0, c"CHASE7"), (300.0, c"CHASE8"), (350.0, c"CHASE9"), (400.0, c"CHASE10"),
    (500.0, c"CHASE11"), (600.0, c"CHASE12"), (700.0, c"CHASE13"), (800.0, c"CHASE14"), (900.0, c"CHASE15"),
    (1000.0, c"CHASE16"), (1200.0, c"CHASE17"), (1400.0, c"CHASE18"), (1600.0, c"CHASE19"), (1800.0, c"CHASE20"),
];

/// What the rating needs from the rest of the game.
#[derive(Clone, Copy, Debug, Default)]
pub struct World {
    /// CWorld::Players[PlayerInFocus].m_nMoney.
    pub money: i32,
    /// .m_nVisibleMoney.
    pub visible_money: i32,
    /// CPad::bHasPlayerCheated.
    pub cheated: bool,
}

pub const SAVE_SIZE: usize = 595;

impl Stats {
    pub const ZERO: Stats = unsafe { std::mem::zeroed() };

    /// Init; `radio` is DMAudio's listen time array (PopulateFavoriteRadioStationList).
    pub fn init(&mut self, radio: &[f32; NUM_RADIOS]) {
        // Init zeroes everything but these two (FIX_BUGS: the plane distance is cleared too).
        let keep = *self;
        *self = Stats::ZERO;
        self.TotalProgressInGame = keep.TotalProgressInGame;
        self.BestTimeBombDefusal = keep.BestTimeBombDefusal;
        self.BestPositions = [i32::MAX; TOTAL_BEST_POSITIONS];
        self.FavoriteRadioStationList = *radio;
    }

    pub fn register_fastest_time(&mut self, index: usize, time: i32) {
        let t = &mut self.FastestTimes[index];
        *t = if *t == 0 { time } else if *t < time { *t } else { time };
    }

    pub fn register_highest_score(&mut self, index: usize, score: i32) {
        let s = &mut self.HighestScores[index];
        *s = if *s > score { *s } else { score };
    }

    pub fn register_best_position(&mut self, index: usize, position: i32) {
        let p = &mut self.BestPositions[index];
        *p = if *p < position { *p } else { position };
    }

    pub fn register_level_ambulance_mission(&mut self, level: i32) {
        self.HighestLevelAmbulanceMission = self.HighestLevelAmbulanceMission.max(level);
    }

    pub fn register_level_vigilante_mission(&mut self, level: i32) {
        self.HighestLevelVigilanteMission = self.HighestLevelVigilanteMission.max(level);
    }

    pub fn register_level_fire_mission(&mut self, level: i32) {
        self.HighestLevelFireMission = self.HighestLevelFireMission.max(level);
    }

    pub fn longest_time_in_blood_ring(&mut self, time: i32) {
        if self.BloodRingTime < time {
            self.BloodRingTime = time;
        }
    }

    pub fn add_property_as_owned(&mut self, id: usize) {
        if !self.PropertyOwned[id] {
            self.PropertyOwned[id] = true;
            self.NumPropertyOwned = self.NumPropertyOwned.wrapping_add(1);
        }
    }

    pub fn check_point_reached_successfully(&mut self) {
        self.TotalLegitimateKills = self.TotalLegitimateKills.wrapping_add(self.KillsSinceLastCheckpoint);
        self.KillsSinceLastCheckpoint = 0;
    }

    pub fn find_criminal_rating_number(&self, w: &World) -> i32 {
        let mut rating = self.FiresExtinguished
            .wrapping_add(self.HighestLevelFireMission.wrapping_mul(10))
            .wrapping_add(self.HighestLevelAmbulanceMission.wrapping_mul(10))
            .wrapping_add(self.CriminalsCaught)
            .wrapping_add(self.LivesSavedWithAmbulance)
            .wrapping_add(self.HelisDestroyed.wrapping_mul(30))
            .wrapping_add(self.TotalLegitimateKills)
            .wrapping_sub(self.TimesArrested.wrapping_mul(3))
            .wrapping_sub(self.TimesDied.wrapping_mul(3))
            .wrapping_add(w.money / 5000);
        if w.cheated || self.CheatedCount > 0 {
            rating = rating.wrapping_sub(self.CheatedCount);
            if rating <= -10000 {
                rating = -10000;
            }
        } else if rating <= 0 {
            rating = 0;
        }
        if self.RoundsFiredByPlayer > 100 {
            rating = f2i(rating as f32 + self.BulletsThatHit as f32 / self.RoundsFiredByPlayer as f32 * 500.0);
        }
        if self.TotalProgressInGame != 0.0 {
            rating = f2i(rating as f32 + self.ProgressMade / self.TotalProgressInGame * 1000.0);
        }
        rating
    }

    /// The GXT key FindCriminalRatingString looks up.
    pub fn find_criminal_rating_key(&self, w: &World) -> &'static CStr {
        let rating = self.find_criminal_rating_number(w);
        if rating < 0 {
            return match rating {
                r if r > -500 => c"RATNG53",
                r if r > -2000 => c"RATNG54",
                r if r > -4000 => c"RATNG55",
                r if r > -6000 => c"RATNG56",
                _ => c"RATNG57",
            };
        }
        if let Some(&(limit, key)) = RATINGS.iter().find(|&&(limit, _)| rating < limit) {
            return match limit {
                1300 if self.TimesArrested <= 0 => c"RATNG24",
                _ => key,
            };
        }
        if rating < 375000 {
            c"RATNG49"
        } else if rating < 500000 {
            if self.FlightTime / 60000 / 60 > 10 { c"RATNG50" } else { c"RATNG49" }
        } else if rating < 1000000 {
            c"RATNG51"
        } else if w.visible_money > 10000000 {
            c"RATNG52"
        } else {
            c"RATNG51"
        }
    }

    pub fn find_chase_key(media_level: f32) -> &'static CStr {
        CHASES.iter().find(|&&(limit, _)| media_level < limit).map_or(c"CHASE21", |&(_, key)| key)
    }

    pub fn get_percentage_progress(&self, nasty_game: bool) -> f32 {
        let p = if self.TotalProgressInGame == 0.0 {
            0.0
        } else {
            100.0 * self.ProgressMade
                / if nasty_game { self.TotalProgressInGame } else { self.TotalProgressInGame - 1.0 }
        };
        if p < 100.0 { p } else { 100.0 }
    }

    /// SaveStats: banks the checkpoint kills and refreshes the radio list first, like the original.
    pub fn save(&mut self, radio: &[f32; NUM_RADIOS]) -> Vec<u8> {
        self.check_point_reached_successfully();
        self.FavoriteRadioStationList = *radio;
        let mut out = Vec::with_capacity(SAVE_SIZE);
        self.write_fields(&mut out);
        debug_assert_eq!(out.len(), SAVE_SIZE);
        out
    }

    /// LoadStats (`buf` holds at least SAVE_SIZE bytes).
    pub fn load(&mut self, buf: &[u8]) {
        self.read_fields(&mut Reader { b: &buf[..SAVE_SIZE] });
    }

    fn write_fields(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.PeopleKilledByPlayer.to_le_bytes());
        out.extend_from_slice(&self.PeopleKilledByOthers.to_le_bytes());
        out.extend_from_slice(&self.CarsExploded.to_le_bytes());
        out.extend_from_slice(&self.BoatsExploded.to_le_bytes());
        out.extend_from_slice(&self.TyresPopped.to_le_bytes());
        out.extend_from_slice(&self.RoundsFiredByPlayer.to_le_bytes());
        for v in &self.PedsKilledOfThisType { out.extend_from_slice(&v.to_le_bytes()); }
        out.extend_from_slice(&self.HelisDestroyed.to_le_bytes());
        out.extend_from_slice(&self.ProgressMade.to_le_bytes());
        out.extend_from_slice(&self.TotalProgressInGame.to_le_bytes());
        out.extend_from_slice(&self.KgsOfExplosivesUsed.to_le_bytes());
        out.extend_from_slice(&self.BulletsThatHit.to_le_bytes());
        out.extend_from_slice(&self.HeadsPopped.to_le_bytes());
        out.extend_from_slice(&self.WantedStarsAttained.to_le_bytes());
        out.extend_from_slice(&self.WantedStarsEvaded.to_le_bytes());
        out.extend_from_slice(&self.TimesArrested.to_le_bytes());
        out.extend_from_slice(&self.TimesDied.to_le_bytes());
        out.extend_from_slice(&self.DaysPassed.to_le_bytes());
        out.extend_from_slice(&self.SafeHouseVisits.to_le_bytes());
        out.extend_from_slice(&self.Sprayings.to_le_bytes());
        out.extend_from_slice(&self.MaximumJumpDistance.to_le_bytes());
        out.extend_from_slice(&self.MaximumJumpHeight.to_le_bytes());
        out.extend_from_slice(&self.MaximumJumpFlips.to_le_bytes());
        out.extend_from_slice(&self.MaximumJumpSpins.to_le_bytes());
        out.extend_from_slice(&self.BestStuntJump.to_le_bytes());
        out.extend_from_slice(&self.NumberOfUniqueJumpsFound.to_le_bytes());
        out.extend_from_slice(&self.TotalNumberOfUniqueJumps.to_le_bytes());
        out.extend_from_slice(&self.MissionsGiven.to_le_bytes());
        out.extend_from_slice(&self.PassengersDroppedOffWithTaxi.to_le_bytes());
        out.extend_from_slice(&self.MoneyMadeWithTaxi.to_le_bytes());
        out.extend_from_slice(&self.IndustrialPassed.to_le_bytes());
        out.extend_from_slice(&self.CommercialPassed.to_le_bytes());
        out.extend_from_slice(&self.SuburbanPassed.to_le_bytes());
        out.extend_from_slice(&self.PamphletMissionPassed.to_le_bytes());
        out.extend_from_slice(&self.NoMoreHurricanes.to_le_bytes());
        out.extend_from_slice(&self.DistanceTravelledOnFoot.to_le_bytes());
        out.extend_from_slice(&self.DistanceTravelledByCar.to_le_bytes());
        out.extend_from_slice(&self.DistanceTravelledByBike.to_le_bytes());
        out.extend_from_slice(&self.DistanceTravelledByBoat.to_le_bytes());
        out.extend_from_slice(&self.DistanceTravelledByGolfCart.to_le_bytes());
        out.extend_from_slice(&self.DistanceTravelledByHelicoptor.to_le_bytes());
        out.extend_from_slice(&self.DistanceTravelledByPlane.to_le_bytes());
        out.extend_from_slice(&self.LivesSavedWithAmbulance.to_le_bytes());
        out.extend_from_slice(&self.CriminalsCaught.to_le_bytes());
        out.extend_from_slice(&self.FiresExtinguished.to_le_bytes());
        out.extend_from_slice(&self.HighestLevelVigilanteMission.to_le_bytes());
        out.extend_from_slice(&self.HighestLevelAmbulanceMission.to_le_bytes());
        out.extend_from_slice(&self.HighestLevelFireMission.to_le_bytes());
        out.extend_from_slice(&self.PhotosTaken.to_le_bytes());
        out.extend_from_slice(&self.NumberKillFrenziesPassed.to_le_bytes());
        out.extend_from_slice(&self.TotalNumberKillFrenzies.to_le_bytes());
        out.extend_from_slice(&self.TotalNumberMissions.to_le_bytes());
        out.extend_from_slice(&self.FlightTime.to_le_bytes());
        out.extend_from_slice(&self.TimesDrowned.to_le_bytes());
        out.extend_from_slice(&self.SeagullsKilled.to_le_bytes());
        out.extend_from_slice(&self.WeaponBudget.to_le_bytes());
        out.extend_from_slice(&self.FashionBudget.to_le_bytes());
        out.extend_from_slice(&self.LoanSharks.to_le_bytes());
        out.extend_from_slice(&self.StoresKnockedOff.to_le_bytes());
        out.extend_from_slice(&self.MovieStunts.to_le_bytes());
        out.extend_from_slice(&self.Assassinations.to_le_bytes());
        out.extend_from_slice(&self.PizzasDelivered.to_le_bytes());
        out.extend_from_slice(&self.GarbagePickups.to_le_bytes());
        out.extend_from_slice(&self.IceCreamSold.to_le_bytes());
        out.extend_from_slice(&self.TopShootingRangeScore.to_le_bytes());
        out.extend_from_slice(&self.ShootingRank.to_le_bytes());
        out.extend_from_slice(&self.LongestWheelie.to_le_bytes());
        out.extend_from_slice(&self.LongestStoppie.to_le_bytes());
        out.extend_from_slice(&self.Longest2Wheel.to_le_bytes());
        out.extend_from_slice(&self.LongestWheelieDist.to_le_bytes());
        out.extend_from_slice(&self.LongestStoppieDist.to_le_bytes());
        out.extend_from_slice(&self.Longest2WheelDist.to_le_bytes());
        out.extend_from_slice(&self.PropertyBudget.to_le_bytes());
        out.extend_from_slice(&self.AutoPaintingBudget.to_le_bytes());
        out.extend_from_slice(&self.PropertyDestroyed.to_le_bytes());
        out.extend_from_slice(&self.NumPropertyOwned.to_le_bytes());
        out.extend_from_slice(&self.BloodRingKills.to_le_bytes());
        out.extend_from_slice(&self.BloodRingTime.to_le_bytes());
        out.extend(self.PropertyOwned.iter().map(|&b| b as u8));
        out.extend_from_slice(&self.HighestChaseValue.to_le_bytes());
        for v in &self.FastestTimes { out.extend_from_slice(&v.to_le_bytes()); }
        for v in &self.HighestScores { out.extend_from_slice(&v.to_le_bytes()); }
        for v in &self.BestPositions { out.extend_from_slice(&v.to_le_bytes()); }
        out.extend_from_slice(&self.KillsSinceLastCheckpoint.to_le_bytes());
        out.extend_from_slice(&self.TotalLegitimateKills.to_le_bytes());
        out.extend_from_slice(&self.LastMissionPassedName);
        out.extend_from_slice(&self.CheatedCount.to_le_bytes());
        for v in &self.FavoriteRadioStationList { out.extend_from_slice(&v.to_le_bytes()); }
    }

    fn read_fields(&mut self, r: &mut Reader) {
        self.PeopleKilledByPlayer = r.i32();
        self.PeopleKilledByOthers = r.i32();
        self.CarsExploded = r.i32();
        self.BoatsExploded = r.i32();
        self.TyresPopped = r.i32();
        self.RoundsFiredByPlayer = r.i32();
        for v in &mut self.PedsKilledOfThisType { *v = r.i32(); }
        self.HelisDestroyed = r.i32();
        self.ProgressMade = r.f32();
        self.TotalProgressInGame = r.f32();
        self.KgsOfExplosivesUsed = r.i32();
        self.BulletsThatHit = r.i32();
        self.HeadsPopped = r.i32();
        self.WantedStarsAttained = r.i32();
        self.WantedStarsEvaded = r.i32();
        self.TimesArrested = r.i32();
        self.TimesDied = r.i32();
        self.DaysPassed = r.i32();
        self.SafeHouseVisits = r.i32();
        self.Sprayings = r.i32();
        self.MaximumJumpDistance = r.f32();
        self.MaximumJumpHeight = r.f32();
        self.MaximumJumpFlips = r.i32();
        self.MaximumJumpSpins = r.i32();
        self.BestStuntJump = r.i32();
        self.NumberOfUniqueJumpsFound = r.i32();
        self.TotalNumberOfUniqueJumps = r.i32();
        self.MissionsGiven = r.i32();
        self.PassengersDroppedOffWithTaxi = r.i32();
        self.MoneyMadeWithTaxi = r.i32();
        self.IndustrialPassed = r.i32();
        self.CommercialPassed = r.i32();
        self.SuburbanPassed = r.i32();
        self.PamphletMissionPassed = r.i32();
        self.NoMoreHurricanes = r.i32();
        self.DistanceTravelledOnFoot = r.f32();
        self.DistanceTravelledByCar = r.f32();
        self.DistanceTravelledByBike = r.f32();
        self.DistanceTravelledByBoat = r.f32();
        self.DistanceTravelledByGolfCart = r.f32();
        self.DistanceTravelledByHelicoptor = r.f32();
        self.DistanceTravelledByPlane = r.f32();
        self.LivesSavedWithAmbulance = r.i32();
        self.CriminalsCaught = r.i32();
        self.FiresExtinguished = r.i32();
        self.HighestLevelVigilanteMission = r.i32();
        self.HighestLevelAmbulanceMission = r.i32();
        self.HighestLevelFireMission = r.i32();
        self.PhotosTaken = r.i32();
        self.NumberKillFrenziesPassed = r.i32();
        self.TotalNumberKillFrenzies = r.i32();
        self.TotalNumberMissions = r.i32();
        self.FlightTime = r.i32();
        self.TimesDrowned = r.i32();
        self.SeagullsKilled = r.i32();
        self.WeaponBudget = r.f32();
        self.FashionBudget = r.f32();
        self.LoanSharks = r.f32();
        self.StoresKnockedOff = r.f32();
        self.MovieStunts = r.f32();
        self.Assassinations = r.f32();
        self.PizzasDelivered = r.f32();
        self.GarbagePickups = r.f32();
        self.IceCreamSold = r.f32();
        self.TopShootingRangeScore = r.f32();
        self.ShootingRank = r.f32();
        self.LongestWheelie = r.i32();
        self.LongestStoppie = r.i32();
        self.Longest2Wheel = r.i32();
        self.LongestWheelieDist = r.f32();
        self.LongestStoppieDist = r.f32();
        self.Longest2WheelDist = r.f32();
        self.PropertyBudget = r.f32();
        self.AutoPaintingBudget = r.f32();
        self.PropertyDestroyed = r.i32();
        self.NumPropertyOwned = r.i32();
        self.BloodRingKills = r.i32();
        self.BloodRingTime = r.i32();
        for v in &mut self.PropertyOwned { *v = r.bool(); }
        self.HighestChaseValue = r.f32();
        for v in &mut self.FastestTimes { *v = r.i32(); }
        for v in &mut self.HighestScores { *v = r.i32(); }
        for v in &mut self.BestPositions { *v = r.i32(); }
        self.KillsSinceLastCheckpoint = r.i32();
        self.TotalLegitimateKills = r.i32();
        for v in &mut self.LastMissionPassedName { *v = r.u8(); }
        self.CheatedCount = r.i32();
        for v in &mut self.FavoriteRadioStationList { *v = r.f32(); }
    }

    #[cfg(test)]
    /// Every field in definition order, as the recorder prints them.
    pub fn dump(&self) -> String {
        let mut s = String::new();
        { let v = self.SeagullsKilled; s += &format!(" {:}", v); }
        { let v = self.BoatsExploded; s += &format!(" {:}", v); }
        { let v = self.WantedStarsAttained; s += &format!(" {:}", v); }
        { let v = self.WantedStarsEvaded; s += &format!(" {:}", v); }
        { let v = self.DaysPassed; s += &format!(" {:}", v); }
        { let v = self.HeadsPopped; s += &format!(" {:}", v); }
        { let v = self.CommercialPassed; s += &format!(" {:}", v); }
        { let v = self.IndustrialPassed; s += &format!(" {:}", v); }
        { let v = self.SuburbanPassed; s += &format!(" {:}", v); }
        { let v = self.NumberKillFrenziesPassed; s += &format!(" {:}", v); }
        { let v = self.PeopleKilledByOthers; s += &format!(" {:}", v); }
        { let v = self.HelisDestroyed; s += &format!(" {:}", v); }
        for (i, &v) in self.PedsKilledOfThisType.iter().enumerate() { s += &format!("{}{:}", if i == 0 { " " } else { "," }, v); }
        { let v = self.TimesDied; s += &format!(" {:}", v); }
        { let v = self.TimesArrested; s += &format!(" {:}", v); }
        { let v = self.KillsSinceLastCheckpoint; s += &format!(" {:}", v); }
        { let v = self.DistanceTravelledByCar; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.DistanceTravelledByHelicoptor; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.DistanceTravelledByBike; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.DistanceTravelledByBoat; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.DistanceTravelledByPlane; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.DistanceTravelledByGolfCart; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.DistanceTravelledOnFoot; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.FlightTime; s += &format!(" {:}", v); }
        { let v = self.TimesDrowned; s += &format!(" {:}", v); }
        { let v = self.PhotosTaken; s += &format!(" {:}", v); }
        { let v = self.LoanSharks; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.StoresKnockedOff; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.MovieStunts; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.Assassinations; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.PizzasDelivered; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.GarbagePickups; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.IceCreamSold; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.TopShootingRangeScore; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.ShootingRank; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.ProgressMade; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.TotalProgressInGame; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.CarsExploded; s += &format!(" {:}", v); }
        { let v = self.PeopleKilledByPlayer; s += &format!(" {:}", v); }
        { let v = self.MaximumJumpDistance; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.MaximumJumpHeight; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.MaximumJumpFlips; s += &format!(" {:}", v); }
        { let v = self.MaximumJumpSpins; s += &format!(" {:}", v); }
        { let v = self.BestStuntJump; s += &format!(" {:}", v); }
        { let v = self.NumberOfUniqueJumpsFound; s += &format!(" {:}", v); }
        { let v = self.TotalNumberOfUniqueJumps; s += &format!(" {:}", v); }
        { let v = self.PassengersDroppedOffWithTaxi; s += &format!(" {:}", v); }
        { let v = self.MoneyMadeWithTaxi; s += &format!(" {:}", v); }
        { let v = self.MissionsGiven; s += &format!(" {:}", v); }
        { let v = self.MissionsPassed; s += &format!(" {:}", v); }
        for (i, &v) in self.LastMissionPassedName.iter().enumerate() { s += &format!("{}{:}", if i == 0 { " " } else { "," }, v); }
        { let v = self.TotalLegitimateKills; s += &format!(" {:}", v); }
        { let v = self.LivesSavedWithAmbulance; s += &format!(" {:}", v); }
        { let v = self.CriminalsCaught; s += &format!(" {:}", v); }
        { let v = self.HighestLevelAmbulanceMission; s += &format!(" {:}", v); }
        { let v = self.HighestLevelVigilanteMission; s += &format!(" {:}", v); }
        { let v = self.HighestLevelFireMission; s += &format!(" {:}", v); }
        { let v = self.FiresExtinguished; s += &format!(" {:}", v); }
        { let v = self.TotalNumberKillFrenzies; s += &format!(" {:}", v); }
        { let v = self.TotalNumberMissions; s += &format!(" {:}", v); }
        { let v = self.RoundsFiredByPlayer; s += &format!(" {:}", v); }
        { let v = self.KgsOfExplosivesUsed; s += &format!(" {:}", v); }
        { let v = self.BulletsThatHit; s += &format!(" {:}", v); }
        { let v = self.BestTimeBombDefusal; s += &format!(" {:}", v); }
        for (i, &v) in self.FastestTimes.iter().enumerate() { s += &format!("{}{:}", if i == 0 { " " } else { "," }, v); }
        for (i, &v) in self.HighestScores.iter().enumerate() { s += &format!("{}{:}", if i == 0 { " " } else { "," }, v); }
        for (i, &v) in self.BestPositions.iter().enumerate() { s += &format!("{}{:}", if i == 0 { " " } else { "," }, v); }
        for (i, &v) in self.PropertyOwned.iter().enumerate() { s += &format!("{}{:}", if i == 0 { " " } else { "," }, v as u8); }
        { let v = self.NumPropertyOwned; s += &format!(" {:}", v); }
        { let v = self.PropertyDestroyed; s += &format!(" {:}", v); }
        { let v = self.HighestChaseValue; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.CheatedCount; s += &format!(" {:}", v); }
        { let v = self.ShowChaseStatOnScreen; s += &format!(" {:}", v); }
        { let v = self.PamphletMissionPassed; s += &format!(" {:}", v); }
        for (i, &v) in self.abSonyCDs.iter().enumerate() { s += &format!("{}{:}", if i == 0 { " " } else { "," }, v as u8); }
        { let v = self.BloodRingKills; s += &format!(" {:}", v); }
        { let v = self.BloodRingTime; s += &format!(" {:}", v); }
        for (i, &v) in self.FavoriteRadioStationList.iter().enumerate() { s += &format!("{}{:x}", if i == 0 { " " } else { "," }, v.to_bits()); }
        { let v = self.Sprayings; s += &format!(" {:}", v); }
        { let v = self.AutoPaintingBudget; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.NoMoreHurricanes; s += &format!(" {:}", v); }
        { let v = self.FashionBudget; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.PropertyBudget; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.WeaponBudget; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.SafeHouseVisits; s += &format!(" {:}", v); }
        { let v = self.TyresPopped; s += &format!(" {:}", v); }
        { let v = self.LongestWheelie; s += &format!(" {:}", v); }
        { let v = self.LongestStoppie; s += &format!(" {:}", v); }
        { let v = self.Longest2Wheel; s += &format!(" {:}", v); }
        { let v = self.LongestWheelieDist; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.LongestStoppieDist; s += &format!(" {:x}", v.to_bits()); }
        { let v = self.Longest2WheelDist; s += &format!(" {:x}", v.to_bits()); }
        s
    }

    #[cfg(test)]
    /// Test hook: set scalar field `i` (definition order, scalars only) from raw bits.
    pub fn set_scalar(&mut self, i: usize, bits: u32) {
        match i {
            0 => self.SeagullsKilled = bits as i32,
            1 => self.BoatsExploded = bits as i32,
            2 => self.WantedStarsAttained = bits as i32,
            3 => self.WantedStarsEvaded = bits as i32,
            4 => self.DaysPassed = bits as i32,
            5 => self.HeadsPopped = bits as i32,
            6 => self.CommercialPassed = bits as i32,
            7 => self.IndustrialPassed = bits as i32,
            8 => self.SuburbanPassed = bits as i32,
            9 => self.NumberKillFrenziesPassed = bits as i32,
            10 => self.PeopleKilledByOthers = bits as i32,
            11 => self.HelisDestroyed = bits as i32,
            12 => self.TimesDied = bits as i32,
            13 => self.TimesArrested = bits as i32,
            14 => self.KillsSinceLastCheckpoint = bits as i32,
            15 => self.DistanceTravelledByCar = f32::from_bits(bits),
            16 => self.DistanceTravelledByHelicoptor = f32::from_bits(bits),
            17 => self.DistanceTravelledByBike = f32::from_bits(bits),
            18 => self.DistanceTravelledByBoat = f32::from_bits(bits),
            19 => self.DistanceTravelledByPlane = f32::from_bits(bits),
            20 => self.DistanceTravelledByGolfCart = f32::from_bits(bits),
            21 => self.DistanceTravelledOnFoot = f32::from_bits(bits),
            22 => self.FlightTime = bits as i32,
            23 => self.TimesDrowned = bits as i32,
            24 => self.PhotosTaken = bits as i32,
            25 => self.LoanSharks = f32::from_bits(bits),
            26 => self.StoresKnockedOff = f32::from_bits(bits),
            27 => self.MovieStunts = f32::from_bits(bits),
            28 => self.Assassinations = f32::from_bits(bits),
            29 => self.PizzasDelivered = f32::from_bits(bits),
            30 => self.GarbagePickups = f32::from_bits(bits),
            31 => self.IceCreamSold = f32::from_bits(bits),
            32 => self.TopShootingRangeScore = f32::from_bits(bits),
            33 => self.ShootingRank = f32::from_bits(bits),
            34 => self.ProgressMade = f32::from_bits(bits),
            35 => self.TotalProgressInGame = f32::from_bits(bits),
            36 => self.CarsExploded = bits as i32,
            37 => self.PeopleKilledByPlayer = bits as i32,
            38 => self.MaximumJumpDistance = f32::from_bits(bits),
            39 => self.MaximumJumpHeight = f32::from_bits(bits),
            40 => self.MaximumJumpFlips = bits as i32,
            41 => self.MaximumJumpSpins = bits as i32,
            42 => self.BestStuntJump = bits as i32,
            43 => self.NumberOfUniqueJumpsFound = bits as i32,
            44 => self.TotalNumberOfUniqueJumps = bits as i32,
            45 => self.PassengersDroppedOffWithTaxi = bits as i32,
            46 => self.MoneyMadeWithTaxi = bits as i32,
            47 => self.MissionsGiven = bits as i32,
            48 => self.MissionsPassed = bits as i32,
            49 => self.TotalLegitimateKills = bits as i32,
            50 => self.LivesSavedWithAmbulance = bits as i32,
            51 => self.CriminalsCaught = bits as i32,
            52 => self.HighestLevelAmbulanceMission = bits as i32,
            53 => self.HighestLevelVigilanteMission = bits as i32,
            54 => self.HighestLevelFireMission = bits as i32,
            55 => self.FiresExtinguished = bits as i32,
            56 => self.TotalNumberKillFrenzies = bits as i32,
            57 => self.TotalNumberMissions = bits as i32,
            58 => self.RoundsFiredByPlayer = bits as i32,
            59 => self.KgsOfExplosivesUsed = bits as i32,
            60 => self.BulletsThatHit = bits as i32,
            61 => self.BestTimeBombDefusal = bits as i32,
            62 => self.NumPropertyOwned = bits as i32,
            63 => self.PropertyDestroyed = bits as i32,
            64 => self.HighestChaseValue = f32::from_bits(bits),
            65 => self.CheatedCount = bits as i32,
            66 => self.ShowChaseStatOnScreen = bits as i32,
            67 => self.PamphletMissionPassed = bits as i32,
            68 => self.BloodRingKills = bits as i32,
            69 => self.BloodRingTime = bits as i32,
            70 => self.Sprayings = bits as i32,
            71 => self.AutoPaintingBudget = f32::from_bits(bits),
            72 => self.NoMoreHurricanes = bits as i32,
            73 => self.FashionBudget = f32::from_bits(bits),
            74 => self.PropertyBudget = f32::from_bits(bits),
            75 => self.WeaponBudget = f32::from_bits(bits),
            76 => self.SafeHouseVisits = bits as i32,
            77 => self.TyresPopped = bits as i32,
            78 => self.LongestWheelie = bits as i32,
            79 => self.LongestStoppie = bits as i32,
            80 => self.Longest2Wheel = bits as i32,
            81 => self.LongestWheelieDist = f32::from_bits(bits),
            82 => self.LongestStoppieDist = f32::from_bits(bits),
            83 => self.Longest2WheelDist = f32::from_bits(bits),
            _ => panic!("field {i}"),
        }
    }
}

// --- C ABI (C++ CStats forwards here) ---

fn stats() -> &'static mut Stats {
    // SAFETY: game thread only, like the C++ statics it replaces.
    unsafe { &mut *std::ptr::addr_of_mut!(revc_stats) }
}

unsafe fn radio_list(p: *const f32) -> [f32; NUM_RADIOS] {
    unsafe { *(p as *const [f32; NUM_RADIOS]) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_stats_init(radio: *const f32) {
    stats().init(&unsafe { radio_list(radio) });
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_stats_register_fastest_time(index: i32, time: i32) {
    stats().register_fastest_time(index as usize, time);
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_stats_register_highest_score(index: i32, score: i32) {
    stats().register_highest_score(index as usize, score);
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_stats_register_best_position(index: i32, position: i32) {
    stats().register_best_position(index as usize, position);
}

/// RegisterLevel{Ambulance,Vigilante,Fire}Mission (kind 0, 1, 2).
#[unsafe(no_mangle)]
pub extern "C" fn revc_stats_register_level(kind: i32, level: i32) {
    let s = stats();
    match kind {
        0 => s.register_level_ambulance_mission(level),
        1 => s.register_level_vigilante_mission(level),
        _ => s.register_level_fire_mission(level),
    }
}

/// The float budgets and counters (`f += (float)n`): MoneySpentOn{Weapons,Property,AutoPainting,
/// Fashion}, NumOf{VisitsFromLoanSharks,StoresKnockedOff,MovieStunts,Assassinations,
/// PizzasDelivered,GarbagePickups,IceCreamSold}, kinds 0..=10.
#[unsafe(no_mangle)]
pub extern "C" fn revc_stats_add_float(kind: i32, n: i32) {
    let s = stats();
    let f = match kind {
        0 => &mut s.WeaponBudget,
        1 => &mut s.PropertyBudget,
        2 => &mut s.AutoPaintingBudget,
        3 => &mut s.FashionBudget,
        4 => &mut s.LoanSharks,
        5 => &mut s.StoresKnockedOff,
        6 => &mut s.MovieStunts,
        7 => &mut s.Assassinations,
        8 => &mut s.PizzasDelivered,
        9 => &mut s.GarbagePickups,
        _ => &mut s.IceCreamSold,
    };
    *f += n as f32;
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_stats_longest_time_in_blood_ring(time: i32) {
    stats().longest_time_in_blood_ring(time);
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_stats_add_property_as_owned(id: i32) {
    stats().add_property_as_owned(id as usize);
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_stats_rating_number(money: i32, cheated: bool) -> i32 {
    stats().find_criminal_rating_number(&World { money, visible_money: 0, cheated })
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_stats_rating_key(money: i32, visible_money: i32, cheated: bool) -> *const c_char {
    stats().find_criminal_rating_key(&World { money, visible_money, cheated }).as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_stats_chase_key(media_level: f32) -> *const c_char {
    Stats::find_chase_key(media_level).as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_stats_percentage_progress(nasty_game: bool) -> f32 {
    stats().get_percentage_progress(nasty_game)
}

/// SaveStats into `buf` (SAVE_SIZE bytes); returns the size.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_stats_save(buf: *mut u8, radio: *const f32) -> u32 {
    let out = stats().save(&unsafe { radio_list(radio) });
    unsafe { std::ptr::copy_nonoverlapping(out.as_ptr(), buf, out.len()) };
    out.len() as u32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn revc_stats_load(buf: *const u8, size: u32) {
    debug_assert_eq!(size as usize, SAVE_SIZE);
    stats().load(unsafe { std::slice::from_raw_parts(buf, SAVE_SIZE) });
}

#[unsafe(no_mangle)]
pub extern "C" fn revc_stats_size() -> u32 {
    std::mem::size_of::<Stats>() as u32
}

#[cfg(test)]
mod golden {
    //! tests/golden/stats.txt: random op sequences through the original CStats, every static after each.
    use super::*;

    fn radio(t: &[&str]) -> [f32; NUM_RADIOS] {
        std::array::from_fn(|i| f32::from_bits(u32::from_str_radix(t[i], 16).unwrap()))
    }

    #[test]
    fn matches_cpp_golden() {
        let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/stats.txt")).unwrap();
        let mut s = Box::new(Stats::ZERO);
        let mut saved: Vec<u8> = Vec::new();
        let mut n = 0;
        for (ln, line) in text.lines().enumerate() {
            let (lhs, rhs) = line.split_once(" =>").unwrap();
            let (want_res, want_state) = rhs.split_once(" |").unwrap();
            let t: Vec<&str> = lhs.split(' ').collect();
            let i = |k: usize| t[k].parse::<i32>().unwrap();
            let mut res = String::new();
            match t[0] {
                "init" | "0" => s.init(&radio(&t[1..])),
                "1" => s.register_fastest_time(i(1) as usize, i(2)),
                "2" => s.register_highest_score(i(1) as usize, i(2)),
                "3" => s.register_best_position(0, i(1)),
                "4" => match i(1) {
                    0 => s.LivesSavedWithAmbulance = s.LivesSavedWithAmbulance.wrapping_add(1),
                    1 => s.CriminalsCaught = s.CriminalsCaught.wrapping_add(1),
                    2 => s.FiresExtinguished = s.FiresExtinguished.wrapping_add(1),
                    3 => s.NumberKillFrenziesPassed = s.NumberKillFrenziesPassed.wrapping_add(1),
                    _ => s.KillsSinceLastCheckpoint = 0,
                },
                "5" => match i(1) {
                    0 => s.register_level_ambulance_mission(i(2)),
                    1 => s.register_level_vigilante_mission(i(2)),
                    _ => s.register_level_fire_mission(i(2)),
                },
                "6" => {
                    s.TotalNumberKillFrenzies = i(1);
                    s.TotalNumberMissions = i(2);
                }
                "7" => s.check_point_reached_successfully(),
                "8" | "9" => {
                    // Through the same table as the C ABI.
                    let g = std::ptr::addr_of_mut!(revc_stats);
                    unsafe {
                        *g = *s;
                        revc_stats_add_float(i(1), i(2));
                        *s = *g;
                    }
                }
                "10" => s.BloodRingKills = s.BloodRingKills.wrapping_add(i(1)),
                "11" => s.longest_time_in_blood_ring(i(1)),
                "12" => s.add_property_as_owned(i(1) as usize),
                "13" | "14" | "15" => {
                    let w = World { money: i(1), visible_money: i(2), cheated: i(3) != 0 };
                    res = format!(" {} {}", s.find_criminal_rating_number(&w), s.find_criminal_rating_key(&w).to_str().unwrap());
                }
                "16" => {
                    let f = f32::from_bits(u32::from_str_radix(t[1], 16).unwrap());
                    res = format!(" {}", Stats::find_chase_key(f).to_str().unwrap());
                }
                "17" => res = format!(" {:08x}", s.get_percentage_progress(i(1) != 0).to_bits()),
                "18" => {
                    saved = s.save(&radio(&t[1..]));
                    res = format!(" {} {}", saved.len(), saved.iter().map(|b| format!("{b:02x}")).collect::<String>());
                }
                "19" => {
                    if !saved.is_empty() {
                        for m in &t[2..] {
                            let (at, v) = m.split_once(':').unwrap();
                            saved[at.parse::<usize>().unwrap()] = v.parse().unwrap();
                        }
                        s.load(&saved);
                    }
                }
                "20" | "21" => s.set_scalar(i(1) as usize, t[2].parse::<u32>().unwrap()),
                op => panic!("op {op}"),
            }
            assert_eq!(res, want_res, "result, line {}", ln + 1);
            assert_eq!(s.dump(), want_state, "state, line {}", ln + 1);
            n += 1;
        }
        assert!(n > 2000);
    }
}
