//! Voice effects for fun, applied to your microphone before it's sent:
//! a higher or deeper voice (a two-head delay-line pitch shifter) and a
//! walkie-talkie radio (a narrow band-pass and a little overdrive).

/// The effects, by their config name.
pub const EFFECTS: [&str; 4] = ["none", "high", "low", "radio"];

pub fn label(name: &str) -> &'static str {
    match name {
        "high" => "Higher voice",
        "low" => "Deeper voice",
        "radio" => "Radio",
        _ => "None",
    }
}

pub fn next(name: &str) -> &'static str {
    let i = EFFECTS.iter().position(|e| *e == name).unwrap_or(0);
    EFFECTS[(i + 1) % EFFECTS.len()]
}

pub fn prev(name: &str) -> &'static str {
    let i = EFFECTS.iter().position(|e| *e == name).unwrap_or(0);
    EFFECTS[(i + EFFECTS.len() - 1) % EFFECTS.len()]
}

const RATE: f32 = crate::voice::SAMPLE_RATE as f32;

/// RBJ biquad.
#[derive(Clone, Copy, Default)]
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    z: [f32; 2],
}

impl Biquad {
    fn new(kind: u8, freq: f32, q: f32) -> Self {
        let w = 2.0 * std::f32::consts::PI * freq / RATE;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        let (b0, b1, b2) = if kind == 0 {
            // low-pass
            ((1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0)
        } else {
            // high-pass
            ((1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0)
        };
        Self { b: [b0 / a0, b1 / a0, b2 / a0], a: [-2.0 * cos / a0, (1.0 - alpha) / a0], z: [0.0; 2] }
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

const WINDOW: f32 = 960.0;
const LINE: usize = 2048;

/// Two read heads sliding through a delay line at `ratio` speed, crossfaded
/// so each fades out before it wraps.
struct Pitch {
    line: Vec<f32>,
    write: usize,
    offset: f32,
}

impl Pitch {
    fn new() -> Self {
        Self { line: vec![0.0; LINE], write: 0, offset: 0.0 }
    }

    fn read(&self, delay: f32) -> f32 {
        let pos = self.write as f32 - delay;
        let pos = pos.rem_euclid(LINE as f32);
        let i = pos as usize;
        let frac = pos - i as f32;
        let a = self.line[i % LINE];
        let b = self.line[(i + 1) % LINE];
        a + (b - a) * frac
    }

    fn run(&mut self, x: f32, ratio: f32) -> f32 {
        self.line[self.write] = x;
        self.offset = (self.offset + 1.0 - ratio).rem_euclid(WINDOW);
        let d1 = self.offset + 2.0;
        let d2 = (self.offset + WINDOW / 2.0).rem_euclid(WINDOW) + 2.0;
        let g = |d: f32| (std::f32::consts::PI * (d - 2.0) / WINDOW).sin();
        let (g1, g2) = (g(d1), g(d2));
        let y = (self.read(d1) * g1 + self.read(d2) * g2) / (g1 * g1 + g2 * g2).sqrt().max(0.5);
        self.write = (self.write + 1) % LINE;
        y
    }
}

pub struct VoiceFx {
    current: &'static str,
    pitch: Pitch,
    filters: [Biquad; 3],
}

impl Default for VoiceFx {
    fn default() -> Self {
        Self::new()
    }
}

impl VoiceFx {
    pub fn new() -> Self {
        Self { current: "none", pitch: Pitch::new(), filters: [Biquad::default(); 3] }
    }

    /// Runs `effect` over one frame.
    pub fn process(&mut self, effect: &str, frame: &mut [i16]) {
        let effect = EFFECTS.iter().copied().find(|e| *e == effect).unwrap_or("none");
        if effect != self.current {
            self.current = effect;
            self.pitch = Pitch::new();
            self.filters = [Biquad::new(1, 450.0, 0.7), Biquad::new(0, 2600.0, 0.7), Biquad::new(1, 300.0, 0.7)];
        }
        match effect {
            "high" | "low" => {
                let ratio = if effect == "high" { 1.45 } else { 0.72 };
                for s in frame.iter_mut() {
                    let y = self.pitch.run(*s as f32 / 32768.0, ratio);
                    *s = (y.clamp(-1.0, 1.0) * 32767.0) as i16;
                }
            }
            "radio" => {
                for s in frame.iter_mut() {
                    let mut y = *s as f32 / 32768.0;
                    for f in &mut self.filters {
                        y = f.run(y);
                    }
                    y = (y * 3.0).tanh() * 0.6;
                    *s = (y * 32767.0) as i16;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq: f32, n: usize) -> Vec<i16> {
        (0..n).map(|i| ((2.0 * std::f32::consts::PI * freq * i as f32 / RATE).sin() * 8000.0) as i16).collect()
    }

    fn energy(s: &[i16]) -> f64 {
        s.iter().map(|v| (*v as f64).powi(2)).sum()
    }

    #[test]
    fn radio_cuts_the_lows() {
        let mut fx = VoiceFx::new();
        let mut low = tone(80.0, 16000);
        fx.process("radio", &mut low);
        let mut fx = VoiceFx::new();
        let mut mid = tone(1000.0, 16000);
        fx.process("radio", &mut mid);
        assert!(energy(&mid[4000..]) > energy(&low[4000..]) * 20.0);
    }

    /// Zero crossings per second follow the pitch ratio.
    #[test]
    fn pitch_moves_the_tone() {
        let crossings = |s: &[i16]| s.windows(2).filter(|w| (w[0] < 0) != (w[1] < 0)).count();
        let mut fx = VoiceFx::new();
        let mut up = tone(300.0, 32000);
        fx.process("high", &mut up);
        let mut fx = VoiceFx::new();
        let mut down = tone(300.0, 32000);
        fx.process("low", &mut down);
        let base = crossings(&tone(300.0, 16000)) as f32;
        let up = crossings(&up[16000..]) as f32;
        let down = crossings(&down[16000..]) as f32;
        assert!(up > base * 1.25, "{up} vs {base}");
        assert!(down < base * 0.85, "{down} vs {base}");
    }

    #[test]
    fn cycles() {
        assert_eq!(next("radio"), "none");
        assert_eq!(prev("none"), "radio");
        assert_eq!(label("low"), "Deeper voice");
    }
}
