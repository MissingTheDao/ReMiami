//! Pad scripts (plan task 13, research autopad/README.md, local): timed controller input in 60 Hz
//! frames, the same file a virtual pad plays into PCSX2/xemu. revc plays it on pad 0
//! (testdrive.rs, REVC_TEST=padscript) so the console and revc get the same presses.
//!
//! A script is JSON: {"id": .., "style": "ps2"|"xbox", "fps": 60, "start": "menu_base",
//! "steps": ["<frame> op op ...", ...]}. Ops: `+btn` / `-btn` press / release, `btn*N` tap for N
//! frames, `lx=-1` (lx ly rx ry l2 r2 lt rt) analog values, `mark:<label>`.
//! Buttons: PS2 cross circle square triangle l1 r1 l2 r2 l3 r3 select start up down left right;
//! Xbox a b x y white black lt rt back start ls rs (and the d-pad names).

/// Bits of the pad state handed to revc_test_pad (testhooks.cpp), in this order.
pub const BUTTONS: [&str; 16] = [
    "cross", "circle", "square", "triangle", "l1", "r1", "l2", "r2", "l3", "r3", "select", "start", "up", "down", "left", "right",
];

/// The PS2 name for a button (Xbox names are mapped as revc's Xbox style reads them).
fn ps2_name(b: &str) -> Option<&'static str> {
    let b = match b {
        "a" => "cross",
        "b" => "circle",
        "x" => "square",
        "y" => "triangle",
        "back" => "select",
        "ls" => "l3",
        "rs" => "r3",
        "lt" => "l2",
        "rt" => "r2",
        "white" => "l1",
        "black" => "r1",
        o => o,
    };
    BUTTONS.iter().copied().find(|n| *n == b)
}

#[derive(Debug, Clone)]
enum Op {
    Press(u32),
    Release(u32),
    Axis(usize, f32),
    Mark(String),
}

#[derive(Debug, Default)]
pub struct PadScript {
    pub id: String,
    pub style: String,
    pub start: String,
    events: Vec<(u32, Op)>,
    /// Source steps as written (for copying the script out).
    pub steps: Vec<String>,
}

/// The pad state for one frame: button bits and lx ly rx ry l2 r2 in -1..1 / 0..1.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct PadState {
    pub buttons: u32,
    pub axes: [f32; 6],
}

fn json_string(s: &str, key: &str) -> Option<String> {
    let k = format!("\"{key}\"");
    let i = s.find(&k)? + k.len();
    let rest = s[i..].trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    Some(rest[..rest.find('"')?].to_string())
}

impl PadScript {
    pub fn new(id: &str, style: &str, start: &str) -> PadScript {
        PadScript { id: id.into(), style: style.into(), start: start.into(), ..Default::default() }
    }

    pub fn parse(json: &str) -> Result<PadScript, String> {
        let mut p = PadScript {
            id: json_string(json, "id").unwrap_or_default(),
            style: json_string(json, "style").unwrap_or_default(),
            start: json_string(json, "start").unwrap_or_else(|| "menu_base".into()),
            ..Default::default()
        };
        let i = json.find("\"steps\"").ok_or("no \"steps\"")?;
        let a = json[i..].find('[').ok_or("bad steps")? + i + 1;
        let b = json[a..].find(']').ok_or("bad steps")? + a;
        let mut rest = &json[a..b];
        while let Some(q) = rest.find('"') {
            let r = &rest[q + 1..];
            let e = r.find('"').ok_or("unterminated step")?;
            p.add_step(&r[..e])?;
            rest = &r[e + 1..];
        }
        Ok(p)
    }

    pub fn add_step(&mut self, step: &str) -> Result<(), String> {
        let mut it = step.split_whitespace();
        let f: u32 = it.next().and_then(|v| v.parse().ok()).ok_or(format!("step without a frame: {step}"))?;
        for op in it {
            let btn = |n: &str| ps2_name(n).map(|b| BUTTONS.iter().position(|x| *x == b).unwrap() as u32).ok_or(format!("unknown button {n}"));
            if let Some(m) = op.strip_prefix("mark:") {
                self.events.push((f, Op::Mark(m.to_string())));
            } else if let Some(b) = op.strip_prefix('+') {
                self.events.push((f, Op::Press(btn(b)?)));
            } else if let Some(b) = op.strip_prefix('-') {
                self.events.push((f, Op::Release(btn(b)?)));
            } else if let Some((b, n)) = op.split_once('*') {
                let n: u32 = n.parse().map_err(|_| format!("bad tap {op}"))?;
                let b = btn(b)?;
                self.events.push((f, Op::Press(b)));
                self.events.push((f + n, Op::Release(b)));
            } else if let Some((a, v)) = op.split_once('=') {
                let v: f32 = v.parse().map_err(|_| format!("bad value {op}"))?;
                let ax = match a {
                    "lx" => 0,
                    "ly" => 1,
                    "rx" => 2,
                    "ry" => 3,
                    "l2" | "lt" => 4,
                    "r2" | "rt" => 5,
                    _ => return Err(format!("unknown axis {a}")),
                };
                self.events.push((f, Op::Axis(ax, v)));
            } else {
                return Err(format!("unknown op {op}"));
            }
        }
        self.steps.push(step.to_string());
        self.events.sort_by_key(|e| e.0);
        Ok(())
    }

    /// The frame of a mark, if the script has it.
    pub fn mark(&self, label: &str) -> Option<u32> {
        self.events.iter().find_map(|(f, o)| matches!(o, Op::Mark(m) if m == label).then_some(*f))
    }

    /// The last frame anything happens on.
    pub fn last_frame(&self) -> u32 {
        self.events.last().map_or(0, |e| e.0)
    }

    /// The state after applying every event up to and including `frame`.
    pub fn state_at(&self, frame: u32) -> PadState {
        let mut s = PadState::default();
        for (f, op) in &self.events {
            if *f > frame {
                break;
            }
            match op {
                Op::Press(b) => s.buttons |= 1 << b,
                Op::Release(b) => s.buttons &= !(1 << b),
                Op::Axis(a, v) => s.axes[*a] = *v,
                Op::Mark(_) => {}
            }
        }
        s
    }

    pub fn to_json(&self) -> String {
        let steps: Vec<String> = self.steps.iter().map(|s| format!("\"{s}\"")).collect();
        format!(
            "{{\"id\": \"{}\", \"style\": \"{}\", \"fps\": 60, \"start\": \"{}\",\n \"steps\": [{}]}}\n",
            self.id,
            self.style,
            self.start,
            steps.join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_and_play() {
        let p = PadScript::parse(r#"{"id": "t", "style": "ps2", "start": "menu_base", "steps": ["0 start*4", "40 +cross lx=-1", "44 -cross", "50 mark:input"]}"#).unwrap();
        assert_eq!(p.state_at(0).buttons, 1 << 11);
        assert_eq!(p.state_at(4).buttons, 0);
        assert_eq!(p.state_at(41).buttons, 1);
        assert_eq!(p.state_at(41).axes[0], -1.0);
        assert_eq!(p.state_at(44).buttons, 0);
        assert_eq!(p.mark("input"), Some(50));
        assert!(PadScript::parse(r#"{"steps": ["0 a*4 black*2"]}"#).is_ok());
    }
}
