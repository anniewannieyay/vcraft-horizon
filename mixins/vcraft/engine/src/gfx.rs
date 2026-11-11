//! What the glue draws, and the drawing helpers of the Fabric mod's `Gfx`,
//! `Icons` and `GfxCanvas`, over a list of draw commands in GUI pixels.

/// One thing to draw. Colours are `0xAARRGGBB`.
#[derive(Clone, Debug, PartialEq)]
pub enum Draw {
    Fill([f32; 4], u32),
    /// Text at its top left, without a shadow (the Fabric UI draws none).
    Text(String, [f32; 2], u32),
    /// A player's face from the faces atlas (`slot`), into `[x, y, w, h]`.
    Face(usize, [f32; 4]),
    /// Clip what follows to `[x0, y0, x1, y1]` until the matching `PopClip`.
    PushClip([f32; 4]),
    PopClip,
}

pub const TEXT: u32 = 0xFFDB_DEE1;
pub const WHITE: u32 = 0xFFFF_FFFF;
pub const MUTED: u32 = 0xFF94_9BA4;
pub const FAINT: u32 = 0xFF6D_727A;
pub const GREEN: u32 = 0xFF3B_A55C;
pub const GREEN_HI: u32 = 0xFF45_C06B;
pub const LIGHT_GREEN: u32 = 0xFF8F_DDA8;
pub const DISABLED: u32 = 0xFF2D_5E3E;
pub const SHADOW: u32 = 0xFF11_1214;
pub const MENU: u32 = 0xFF23_2428;
pub const SOFT_RED: u32 = 0xFFFF_8A8C;
pub const PINK_RED: u32 = 0xFFFF_B0B0;
pub const RED: u32 = 0xFFED_4245;
pub const RED_HI: u32 = 0xFFF2_5F62;
pub const GOLD: u32 = 0xFFF0_B232;
pub const BLUE: u32 = 0xFF58_65F2;
pub const PANEL: u32 = 0xFF2B_2D31;
pub const RAISED: u32 = 0xFF38_3A40;
pub const HOVER: u32 = 0xFF40_4249;
pub const SELECTED: u32 = 0xFF4A_4D55;
pub const LINE: u32 = 0xFF26_282C;
pub const BG: u32 = 0xFF1E_1F22;
pub const MAIN: u32 = 0xFF31_3338;
pub const DARK: u32 = 0xFF23_2428;

pub struct Gfx<'a> {
    pub out: Vec<Draw>,
    measure: &'a dyn Fn(&str) -> f32,
    clips: Vec<[i32; 4]>,
}

impl<'a> Gfx<'a> {
    pub fn new(measure: &'a dyn Fn(&str) -> f32) -> Self {
        Self { out: Vec::new(), measure, clips: Vec::new() }
    }

    pub fn width(&self, s: &str) -> i32 {
        (self.measure)(s).round() as i32
    }

    pub fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32) {
        if w > 0 && h > 0 && color >> 24 != 0 {
            self.out.push(Draw::Fill([x as f32, y as f32, w as f32, h as f32], color));
        }
    }

    /// A rectangle with its corners cut (the Fabric mod's rounded look).
    pub fn panel(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32) {
        if w >= 3 && h >= 3 {
            self.fill(x + 1, y, w - 2, h, color);
            self.fill(x, y + 1, 1, h - 2, color);
            self.fill(x + w - 1, y + 1, 1, h - 2, color);
        } else {
            self.fill(x, y, w, h, color);
        }
    }

    pub fn outline(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32) {
        self.fill(x, y, w, 1, color);
        self.fill(x, y + h - 1, w, 1, color);
        self.fill(x, y + 1, 1, h - 2, color);
        self.fill(x + w - 1, y + 1, 1, h - 2, color);
    }

    pub fn text(&mut self, s: &str, x: i32, y: i32, color: u32) {
        if !s.is_empty() {
            self.out.push(Draw::Text(s.to_owned(), [x as f32, y as f32], color));
        }
    }

    pub fn centered(&mut self, s: &str, cx: i32, y: i32, color: u32) {
        let w = self.width(s);
        self.text(s, cx - w / 2, y, color);
    }

    /// `s` cut to `max` pixels with "..." when it doesn't fit.
    pub fn clip(&self, s: &str, max: i32) -> String {
        if self.width(s) <= max {
            return s.to_owned();
        }
        let room = (max - self.width("...")).max(0);
        let mut out = String::new();
        for c in s.chars() {
            out.push(c);
            if self.width(&out) > room {
                out.pop();
                break;
            }
        }
        out + "..."
    }

    /// Word-wrapped lines at most `max` pixels wide (`Font.split`).
    pub fn split(&self, text: &str, max: i32) -> Vec<String> {
        let max = max.max(10);
        let mut lines = Vec::new();
        for para in text.split('\n') {
            let mut cur = String::new();
            for word in para.split_inclusive(' ') {
                let candidate = format!("{cur}{word}");
                if self.width(candidate.trim_end()) <= max || cur.is_empty() {
                    cur = candidate;
                } else {
                    lines.push(cur.trim_end().to_owned());
                    cur = word.to_owned();
                }
                while self.width(cur.trim_end()) > max && cur.chars().count() > 1 {
                    let mut head = String::new();
                    for c in cur.chars() {
                        head.push(c);
                        if self.width(&head) > max {
                            head.pop();
                            break;
                        }
                    }
                    if head.is_empty() {
                        break;
                    }
                    let rest = cur[head.len()..].to_owned();
                    lines.push(head);
                    cur = rest;
                }
            }
            lines.push(cur.trim_end().to_owned());
        }
        lines
    }

    /// A paragraph, ten pixels a line; returns the y below it.
    pub fn para(&mut self, text: &str, x: i32, mut y: i32, max: i32, color: u32) -> i32 {
        for line in self.split(text, max.max(40)) {
            self.text(&line, x, y, color);
            y += 10;
        }
        y
    }

    pub fn scissor(&mut self, x: i32, y: i32, w: i32, h: i32) {
        let mut r = [x, y, x + w, y + h];
        if let Some(outer) = self.clips.last() {
            r = [r[0].max(outer[0]), r[1].max(outer[1]), r[2].min(outer[2]), r[3].min(outer[3])];
        }
        self.clips.push(r);
        self.out.push(Draw::PushClip([r[0] as f32, r[1] as f32, r[2].max(r[0]) as f32, r[3].max(r[1]) as f32]));
    }

    pub fn end_scissor(&mut self) {
        if self.clips.pop().is_some() {
            self.out.push(Draw::PopClip);
        }
    }

    pub fn finish(mut self) -> Vec<Draw> {
        while self.clips.pop().is_some() {
            self.out.push(Draw::PopClip);
        }
        self.out
    }

    /// A player's face, or a coloured initial while the skin loads.
    pub fn face(&mut self, uuid: &str, name: &str, x: i32, y: i32, size: i32) {
        match crate::skins::slot(uuid) {
            Some(slot) => self.out.push(Draw::Face(slot, [x as f32, y as f32, size as f32, size as f32])),
            None => {
                let color = AVATAR_COLORS[(crate::util::java_hash(if uuid.is_empty() { name } else { uuid }).wrapping_mul(31)).rem_euclid(8) as usize];
                self.fill(x, y, size, size, color);
                let initial: String = name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
                if size >= 9 && !initial.is_empty() {
                    let w = self.width(&initial);
                    self.text(&initial, x + (size - w) / 2 + 1, y + (size - 8) / 2, WHITE);
                }
            }
        }
    }

    pub fn icon(&mut self, icon: &[&str], x: i32, y: i32, color: u32) {
        for (row, line) in icon.iter().enumerate() {
            let bytes = line.as_bytes();
            let mut start: Option<usize> = None;
            for col in 0..=bytes.len() {
                let on = col < bytes.len() && bytes[col] == b'#';
                if on && start.is_none() {
                    start = Some(col);
                }
                if !on {
                    if let Some(s) = start.take() {
                        self.fill(x + s as i32, y + row as i32, (col - s) as i32, 1, color);
                    }
                }
            }
        }
    }

    pub fn icon_struck(&mut self, icon: &[&str], x: i32, y: i32, color: u32) {
        self.icon(icon, x, y, color);
        self.icon(&SLASH, x, y, RED);
    }

    pub fn logo(&mut self, x: i32, y: i32) {
        for (row, line) in LOGO.iter().enumerate() {
            let t = row as f32 / 15.0;
            let bg = 0xFF00_0000 | ((82.0 - 52.0 * t) as u32) << 16 | ((242.0 - 103.0 * t) as u32) << 8 | (26.0 + 17.0 * t) as u32;
            for (col, ch) in line.chars().enumerate() {
                let color = match ch {
                    'F' | 'M' => 0xFFD3_EFC8,
                    'G' => bg,
                    'O' => 0xFF17_631F,
                    _ => 0,
                };
                self.fill(x + col as i32, y + row as i32, 1, 1, color);
            }
        }
    }
}

pub const AVATAR_COLORS: [u32; 8] = [0xFF58_65F2, 0xFF3B_A55C, 0xFFED_4245, 0xFFF0_B232, 0xFFEB_459E, 0xFF1A_BC9C, 0xFFE6_7E22, 0xFF9B_59B6];

pub type Icon = [&'static str; 9];

pub const MIC: Icon = ["...###...", "..#####..", "..#####..", "..#####..", ".#.###.#.", ".#.....#.", "..#####..", "....#....", "..#####.."];
pub const HEADPHONES: Icon = ["..#####..", ".#.....#.", "#.......#", "#.......#", "##.....##", "###...###", "###...###", "##.....##", "........."];
pub const GEAR: Icon = ["...#.#...", ".#.###.#.", "..#...#..", "##.....##", ".#..#..#.", "##.....##", "..#...#..", ".#.###.#.", "...#.#..."];
pub const PHONE: Icon = ["##.......", "###......", "###......", ".##......", "..##.....", "...##.##.", "....#####", "......###", "........."];
pub const HANGUP: Icon = [".........", ".........", "..#####..", ".#######.", "###...###", "##.....##", ".........", ".........", "........."];
pub const PLUS: Icon = ["....#....", "....#....", "....#....", "....#....", "#########", "....#....", "....#....", "....#....", "....#...."];
pub const CLOSE: Icon = ["#.......#", ".#.....#.", "..#...#..", "...#.#...", "....#....", "...#.#...", "..#...#..", ".#.....#.", "#.......#"];
pub const CHECK: Icon = [".........", "........#", ".......#.", "......#..", "#....#...", ".#..#....", "..##.....", ".........", "........."];
pub const REPLY: Icon = ["...#.....", "..##.....", ".#######.", "..##....#", "...#....#", "........#", "........#", ".........", "........."];
pub const TRASH: Icon = ["...###...", "#########", ".#.....#.", ".#.#.#.#.", ".#.#.#.#.", ".#.#.#.#.", ".#.#.#.#.", ".#######.", "........."];
pub const PEOPLE: Icon = ["...###...", "..#...#..", "..#...#..", "...###...", ".........", ".#######.", "#.......#", "#.......#", "#########"];
pub const SERVER: Icon = ["#########", "#.......#", "#.##..#.#", "#########", "#.......#", "#.##..#.#", "#########", ".........", "........."];
pub const GLOBE: Icon = ["..#####..", ".#..#..#.", "#..#.#..#", "#########", "#..#.#..#", "#########", "#..#.#..#", ".#..#..#.", "..#####.."];
pub const REFRESH: Icon = ["...####.#", "..#....##", ".#....###", "#........", "#........", "#.......#", ".#.....#.", "..#####..", "........."];
pub const NOTE: Icon = ["...######", "...######", "...#....#", "...#....#", "...#....#", ".###..###", "####.####", "####.####", ".##...##."];
pub const HEART: Icon = [".........", ".##...##.", "####.####", "#########", "#########", ".#######.", "..#####..", "...###...", "....#...."];
pub const LOCK: Icon = ["..#####..", ".#.....#.", ".#.....#.", "#########", "####.####", "####.####", "#########", "#########", "........."];
pub const DOTS: Icon = [".........", ".........", ".........", ".........", "##..#..##", "##..#..##", ".........", ".........", "........."];
pub const SLASH: Icon = ["#........", ".#.......", "..#......", "...#.....", "....#....", ".....#...", "......#..", ".......#.", "........#"];
pub const LEFT: Icon = [".....#...", "....#....", "...#.....", "..#......", ".#.......", "..#......", "...#.....", "....#....", ".....#..."];
pub const RIGHT: Icon = ["...#.....", "....#....", ".....#...", "......#..", ".......#.", "......#..", ".....#...", "....#....", "...#....."];
pub const PLAY: Icon = [".#.......", ".###.....", ".#####...", ".#######.", ".#######.", ".#####...", ".###.....", ".#.......", "........."];
pub const STOP: Icon = [".........", ".#######.", ".#######.", ".#######.", ".#######.", ".#######.", ".#######.", ".#######.", "........."];

const LOGO: [&str; 16] = [
    "..GGGGGGGGGGGG..",
    ".GGGGGGGGGGGGGG.",
    "GGFFFFFFFFFFGGGG",
    "GGFFFFFFFFFFGGGG",
    "GGFGGFFFFGGFGGGG",
    "GGFGGFFFFGGFGGGG",
    "GGFFFFGGFFFFGGGG",
    "GGFFFGGGGFFFOOOG",
    "GGFFFGGGGFFOMMMO",
    "GGFFFGFFGFFOMMMO",
    "GGFFFFFFFFFOMMMO",
    "GGFFFFFFFFFOMMMO",
    "GGGGGGGGGGGGOOOG",
    "GGGGGGGGGGGGGMGG",
    ".GGGGGGGGGGGMMM.",
    "..GGGGGGGGGGGG..",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_and_clips() {
        let m = |s: &str| s.chars().count() as f32 * 6.0;
        let g = Gfx::new(&m);
        assert_eq!(g.split("aa bb cc", 30), vec!["aa bb", "cc"]);
        assert_eq!(g.split("a\nb", 100), vec!["a", "b"]);
        assert_eq!(g.clip("abcdefghij", 40), "abc...");
        let mut g = Gfx::new(&m);
        g.scissor(0, 0, 10, 10);
        g.scissor(5, 5, 10, 10);
        assert_eq!(g.out[1], Draw::PushClip([5.0, 5.0, 10.0, 10.0]));
        assert_eq!(g.finish().len(), 4);
    }
}
