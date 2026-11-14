//! The Xbox's gameplay prompts (plan R4): in the Xbox UI style, a help line whose Xbox sentence
//! names the same presses as the PC's takes the Xbox wording, with each of the Xbox's action codes
//! replaced by the PC action it stands for. The codes are written `~k~~*NAME~`: the prompt filler
//! (Messages.cpp) prints the live binding of NAME, in the Xbox's button words on an Xbox-type pad.
//!
//! A key qualifies when its codes pair up one to one with the PC's and every pair is listed in
//! SYN, when its single Xbox stick code stands for the PC's pair of directions (AXIS), or when it
//! was reviewed by hand (ALLOW). Everything else keeps the PC sentence; see docs/CONSOLE_STYLES.md.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rule {
    Synonyms,
    Axis,
    Allowed,
}

const TILDE: u16 = b'~' as u16;

/// Every `~k~~NAME~` in `s` (NAME of A-Z, 0-9, _): its start, one past its end, and NAME.
pub(crate) fn codes(s: &[u16]) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if s[i..].starts_with(&[TILDE, b'k' as u16, TILDE, TILDE]) {
            let start = i + 4;
            let len = s[start..]
                .iter()
                .take_while(|&&c| u8::try_from(c).is_ok_and(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'))
                .count();
            if len > 0 && s.get(start + len) == Some(&TILDE) {
                let name = String::from_utf16_lossy(&s[start..start + len]);
                out.push((i, start + len + 1, name));
                i = start + len + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn listed(table: &'static [(&'static str, &'static [&'static str])], key: &str) -> Option<&'static [&'static str]> {
    table.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

/// Whether `key` takes the Xbox wording, and by which rule.
pub(crate) fn decide(key: &str, console_codes: &[&str], pc_codes: &[&str]) -> Option<Rule> {
    if console_codes.is_empty() || pc_codes.is_empty() {
        return None;
    }
    let same_len = console_codes.len() == pc_codes.len();
    if same_len && console_codes.iter().zip(pc_codes).all(|(c, p)| listed(SYN, c).is_some_and(|v| v.contains(p))) {
        return Some(Rule::Synonyms);
    }
    if let [c] = console_codes
        && listed(AXIS, c) == Some(pc_codes)
    {
        return Some(Rule::Axis);
    }
    if same_len && ALLOW.contains(&key) {
        return Some(Rule::Allowed);
    }
    None
}

/// The Xbox sentence for `key` with the PC's actions substituted (`~k~~*NAME~`); None: the PC
/// sentence stays.
pub(crate) fn xbox_prompt(key: &str, console: &[u16], pc: &[u16]) -> Option<Vec<u16>> {
    let c = codes(console);
    let p = codes(pc);
    let cn: Vec<&str> = c.iter().map(|x| x.2.as_str()).collect();
    let pn: Vec<&str> = p.iter().map(|x| x.2.as_str()).collect();
    let rule = decide(key, &cn, &pn)?;
    let code = |name: &str| -> Vec<u16> { format!("~k~~*{name}~").encode_utf16().collect() };
    let mut out = Vec::with_capacity(console.len() + 16);
    let mut at = 0;
    for (i, (start, end, _)) in c.iter().enumerate() {
        out.extend_from_slice(&console[at..*start]);
        if rule == Rule::Axis {
            out.extend(code(pn[0]));
            out.push(b'/' as u16);
            out.extend(code(pn[1]));
        } else {
            out.extend(code(pn[i]));
        }
        at = *end;
    }
    out.extend_from_slice(&console[at..]);
    Some(out)
}

const SYN: &[(&str, &[&str])] = &[
    ("FOOT_WEAPON_FIRE", &["PED_FIREWEAPON"]), ("SNIPE_FIRE", &["PED_FIREWEAPON"]),
    ("CAR_SUBMISSION", &["TOGGLE_SUBMISSIONS"]), ("CAR_HORN", &["VEHICLE_HORN"]),
    ("FOOT_WEAPON_TARGET", &["PED_LOCK_TARGET"]), ("TARGET", &["PED_LOCK_TARGET"]),
    ("CAR_BOMB", &["VEHICLE_FIREWEAPON"]), ("CAR_WEAPON_FIRE", &["VEHICLE_FIREWEAPON"]), ("BOAT_WEAPON_FIRE", &["VEHICLE_FIREWEAPON"]),
    ("RCHELI_WEAPON_FIRE", &["VEHICLE_FIREWEAPON"]), ("RCCAR_WEAPON_FIRE", &["VEHICLE_FIREWEAPON"]), ("RCPLANE_WEAPON_FIRE", &["VEHICLE_FIREWEAPON"]),
    ("BIKE_WEAPON_FIRE", &["VEHICLE_FIREWEAPON"]), ("PLANE_WEAPON_FIRE", &["VEHICLE_FIREWEAPON"]),
    ("SNIPE_ZOOM_IN", &["PED_SNIPER_ZOOM_IN"]), ("SNIPE_ZOOM_OUT", &["PED_SNIPER_ZOOM_OUT"]),
    ("CAR_MOVEMENT_ACCELERATE", &["VEHICLE_ACCELERATE"]), ("HELI_MOVEMENT_ACCELERATE", &["VEHICLE_ACCELERATE"]), ("PLANE_MOVEMENT_ACCELERATE", &["VEHICLE_ACCELERATE"]),
    ("CAR_MOVEMENT_BRAKE", &["VEHICLE_BRAKE"]), ("HELI_MOVEMENT_BRAKE", &["VEHICLE_BRAKE"]),
    ("CAR_MOVEMENT_HANDBRAKE", &["VEHICLE_HANDBRAKE"]), ("FOOT_MOVEMENT_DUCK", &["PED_DUCK"]),
    ("FOOT_ENTER_EXIT_VEHICLE", &["VEHICLE_ENTER_EXIT"]), ("FOOT_MOVEMENT_SPRINT", &["PED_SPRINT"]),
    ("FOOT_CAMERA_CYCLE_MODE", &["CAMERA_CHANGE_VIEW_ALL_SITUATIONS"]),
    ("FOOT_WEAPON_SHIFT_TARGET_RIGHT", &["PED_CYCLE_TARGET_RIGHT"]), ("FOOT_WEAPON_SHIFT_TARGET_LEFT", &["PED_CYCLE_TARGET_LEFT"]),
    ("ANSWER_PHONE", &["PED_ANSWER_PHONE"]), ("FOOT_WEAPON_CYCLE_PREVIOUS", &["PED_CYCLE_WEAPON_LEFT"]), ("FOOT_WEAPON_CYCLE_NEXT", &["PED_CYCLE_WEAPON_RIGHT"]),
    ("LOOK_BEHIND_FOR_PED", &["PED_LOOKBEHIND"]), ("FOOT_CAMERA_LOOK_BACK", &["PED_LOOKBEHIND"]),
    ("CAR_RADIO_CHANGE", &["VEHICLE_CHANGE_RADIO_STATION"]), ("FOOT_CAMERA_FORCE_BEHIND_PLAYER", &["PED_CENTER_CAMERA_BEHIND_PLAYER"]),
];

const AXIS: &[(&str, &[&str])] = &[
    ("CAR_MOVEMENT_LEFT_RIGHT", &["GO_LEFT", "GO_RIGHT"]), ("FOOT_MOVEMENT_LEFT_RIGHT", &["GO_LEFT", "GO_RIGHT"]),
    ("CAR_MOVEMENT_UP_DOWN", &["VEHICLE_TURRETUP", "VEHICLE_TURRETDOWN"]), ("CAR_CAMERA_LOOK_LEFT_RIGHT", &["VEHICLE_LOOKLEFT", "VEHICLE_LOOKRIGHT"]),
];

/// Keys whose codes differ by name but mean the same press in that sentence (each reviewed:
/// the property and pickup prompts, the shop exit, Ammu-Nation buy and exit in the PC's order).
const ALLOW: &[&str] = &[
    "BANK_C", "BANK_L", "BANK_T", "BOAT_C", "BOAT_L", "BOAT_T", "CAR_C", "CAR_L",
    "CAR_T", "HYCO_C", "HYCO_L", "HYCO_T", "ICE_C", "ICE_L", "ICE_T", "LNKV_C",
    "LNKV_L", "LNKV_T", "NBMN_C", "NBMN_L", "NBMN_T", "OCHE_C", "OCHE_L", "OCHE_T",
    "PORN_C", "PORN_L", "PORN_T", "PRNT_C", "PRNT_L", "PRNT_T", "SKUM_C", "SKUM_L",
    "SKUM_T", "STRP_C", "STRP_L", "STRP_T", "TAXI_C", "TAXI_L", "TAXI_T", "VCPT_C",
    "VCPT_L", "VCPT_T", "WASH_C", "WASH_L", "WASH_T", "PU_CF1", "PU_CF3", "PU_CF4",
    "EXIT_1", "GUN_H1"
];

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn test_synonyms() {
        let key = "HORN1";
        let console = w("Honk: ~h~~k~~CAR_HORN~");
        let pc = w("Use ~k~~VEHICLE_HORN~ here");
        let result = xbox_prompt(key, &console, &pc);
        assert_eq!(result, Some(w("Honk: ~h~~k~~*VEHICLE_HORN~")));
    }

    #[test]
    fn test_axis() {
        let key = "HELP35";
        let console = w("Turn: ~k~~CAR_MOVEMENT_LEFT_RIGHT~");
        let pc = w("~k~~GO_LEFT~ and ~k~~GO_RIGHT~");
        let result = xbox_prompt(key, &console, &pc);
        assert_eq!(result, Some(w("Turn: ~k~~*GO_LEFT~/~k~~*GO_RIGHT~")));
    }

    #[test]
    fn test_allowed_two_codes() {
        let key = "GUN_H1";
        let console = w("One: ~k~~WEAPON_AMMO_BUY~ two: ~k~~WEAPON_AMMO_EXIT~");
        let pc = w("~k~~PED_SPRINT~ a ~k~~VEHICLE_ENTER_EXIT~ b");
        let result = xbox_prompt(key, &console, &pc);
        assert_eq!(result, Some(w("One: ~k~~*PED_SPRINT~ two: ~k~~*VEHICLE_ENTER_EXIT~")));
    }

    #[test]
    fn test_not_allowed() {
        let key = "HELP37";
        let console = w("x ~k~~FOOT_MOVEMENT_LEFT_RIGHT~");
        let pc = w("y ~k~~PED_SPRINT~");
        let result = xbox_prompt(key, &console, &pc);
        assert_eq!(result, None);
    }

    #[test]
    fn test_swapped_order_not_allowed() {
        let key = "HELP58";
        let console = w("~k~~FOOT_WEAPON_SHIFT_TARGET_RIGHT~ ~k~~FOOT_WEAPON_SHIFT_TARGET_LEFT~");
        let pc = w("~k~~PED_CYCLE_TARGET_LEFT~ ~k~~PED_CYCLE_TARGET_RIGHT~");
        let result = xbox_prompt(key, &console, &pc);
        assert_eq!(result, None);
    }

    #[test]
    fn test_count_mismatch() {
        let key = "BANK_C";
        let console = w("~k~~FOOT_PICKUP~");
        let pc = w("~k~~CODE1~ ~k~~CODE2~");
        let result = xbox_prompt(key, &console, &pc);
        assert_eq!(result, None);
    }

    #[test]
    fn test_no_codes_console() {
        let key = "KEY1";
        let console = w("abc");
        let pc = w("~k~~CODE1~");
        let result = xbox_prompt(key, &console, &pc);
        assert_eq!(result, None);
    }

    #[test]
    fn test_no_codes_pc() {
        let key = "KEY1";
        let console = w("~k~~CODE1~");
        let pc = w("abc");
        let result = xbox_prompt(key, &console, &pc);
        assert_eq!(result, None);
    }

    #[test]
    fn test_codes_valid() {
        let s = w("~k~~A_1~x~k~~~");
        let c = codes(&s);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].2, "A_1");
        assert_eq!(c[0].0, 0);
        assert_eq!(c[0].1, 8)
    }

    #[test]
    fn test_codes_lowercase_invalid() {
        let s = w("~k~~ab~");
        let c = codes(&s);
        assert_eq!(c.len(), 0);
    }

    /// The keys the rules (SYN, AXIS) accept on the real text, pinned: a table change that lets
    /// another key take the Xbox wording fails here until it is reviewed.
    const AUTO: &[&str] = &[
        "ANSWER", "ATUTOR", "ATUTOR3", "CTUTOR", "CTUTOR2", "FTUTOR", "FTUTOR2", "GA_6", "GA_6B",
        "GA_7", "GA_7B", "HELP21", "HELP26", "HELP27", "HELP28", "HELP2A2", "HELP2_A", "HELP31",
        "HELP32", "HELP33", "HELP35", "HELP36", "HELP45", "HELP4_A", "HELP55", "HELP56", "HELP57",
        "HELP5_A", "HELP6_A", "HELP6_C", "HELP6_D", "HELP7_A", "HELP7_D", "HELP8_A", "HELP8_B", "HELP9_A",
        "HELP9_B", "HELP9_C", "HLPSN_B", "HLPSN_C", "HLPSN_D", "HLPSN_E", "HLPSN_F", "HLPSN_G", "HORN1",
        "HORN2", "HORN3", "PBOAT_1", "PBOAT_2", "RCR1_3", "SEG3_4", "TTUTOR", "TTUTOR2",
    ];

    /// On the imported Xbox text and the PC's (both local, never in the repo; skipped without them),
    /// exactly AUTO and ALLOW take the Xbox wording, and the reviewed exceptions keep the PC's.
    #[test]
    fn real_text_takes_exactly_the_reviewed_keys() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let xbox = std::env::var_os("REVC_IMPORT_DIR")
            .map_or_else(|| root.join("../revc-scratch/import"), std::path::PathBuf::from)
            .join("xbox/text/american.gxt");
        let pc = root.join("game/TEXT/american.gxt");
        let (Ok(x), Ok(p)) = (std::fs::read(&xbox), std::fs::read(&pc)) else {
            eprintln!("skipped: no {} or {}", xbox.display(), pc.display());
            return;
        };
        let main = |d: &[u8]| -> std::collections::HashMap<String, Vec<u16>> {
            let g = revc_formats::gxt::Gxt::parse(d).unwrap();
            g.tables.into_iter().find(|t| t.name == "MAIN").unwrap().entries.into_iter().collect()
        };
        let (x, p) = (main(&x), main(&p));
        let mut taken: Vec<&str> = x
            .iter()
            .filter(|(k, xs)| p.get(*k).is_some_and(|ps| xbox_prompt(k, xs, ps).is_some()))
            .map(|(k, _)| k.as_str())
            .collect();
        taken.sort_unstable();
        let mut want: Vec<&str> = AUTO.iter().chain(ALLOW).copied().collect();
        want.sort_unstable();
        assert_eq!(taken, want);
        assert_eq!(taken.len(), 103);
        for k in ["HELP37", "HELP60", "HELP50", "HELP51", "HELP52", "HELP58", "HELP59", "BOLLOX", "HELP15", "HELP53", "PLANE_H"] {
            assert!(!taken.contains(&k), "{k} must keep the PC text");
        }
        // every code in a built sentence is a PC action of the PC sentence
        for k in &taken {
            let built = xbox_prompt(k, &x[*k], &p[*k]).unwrap();
            let pc_names: Vec<String> = codes(&p[*k]).into_iter().map(|c| c.2).collect();
            for (_, _, n) in codes(&built) {
                panic!("{k}: unmarked code {n} left");
            }
            let text = String::from_utf16_lossy(&built);
            for part in text.split("~k~~*").skip(1) {
                let name = &part[..part.find('~').unwrap()];
                assert!(pc_names.iter().any(|p| p == name), "{k}: {name} isn't in the PC sentence");
            }
        }
    }
}
