//! Remembers the windowed-mode size, position and maximized state across launches
//! (the game itself only stores fullscreen vs windowed, in reVC.ini).
//! Stored as key=value lines in userfiles/revc_window.ini, relative to the game directory.

use std::fs;

const PATH: &str = "userfiles/revc_window.ini";

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Geometry {
    /// None until the window has been placed once; Wayland never reports positions.
    pub pos: Option<(i32, i32)>,
    pub size: Option<(i32, i32)>,
    pub maximized: bool,
}

impl Geometry {
    pub fn load() -> Self {
        let mut g = Geometry::default();
        let Ok(text) = fs::read_to_string(PATH) else { return g };
        let (mut x, mut y, mut w, mut h) = (None, None, None, None);
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "x" => x = v.parse().ok(),
                "y" => y = v.parse().ok(),
                "width" => w = v.parse::<i32>().ok().filter(|&n| n >= 320),
                "height" => h = v.parse::<i32>().ok().filter(|&n| n >= 240),
                "maximized" => g.maximized = v == "1",
                _ => {}
            }
        }
        g.pos = x.zip(y);
        g.size = w.zip(h);
        g
    }

    pub fn save(&self) {
        let mut s = String::from("# reVC window state, written by the Rust host\n");
        if let Some((x, y)) = self.pos {
            s += &format!("x={x}\ny={y}\n");
        }
        if let Some((w, h)) = self.size {
            s += &format!("width={w}\nheight={h}\n");
        }
        s += &format!("maximized={}\n", self.maximized as i32);
        if let Err(e) = fs::write(PATH, s) {
            eprintln!("revc: could not save {PATH}: {e}");
        }
    }
}
