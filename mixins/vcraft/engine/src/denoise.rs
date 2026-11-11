//! Microphone noise suppression. Removes steady background noise (fans, hum,
//! hiss, room rumble) from the 20 ms microphone frames before the voice
//! activity gate and the encoder, so the gate stops opening on noise and
//! friends hear less of it.
//!
//! A short-time spectral Wiener filter: 40 ms sqrt-Hann windows at 50%
//! overlap, a noise floor per frequency taken from the quietest moments of
//! the last ~1.5 s (minimum statistics), and a decision-directed gain with a
//! floor so the noise left over stays smooth instead of chirping. Pure Rust,
//! no new crates; adds one frame (20 ms) of delay.

use crate::voice::FRAME_SAMPLES;

const HOP: usize = FRAME_SAMPLES;
const WIN: usize = 2 * HOP;
const BINS: usize = WIN / 2 + 1;
/// Smoothing of the power spectrum the noise floor is tracked on: quick to
/// fall, so the gaps between words reach the floor, slow to rise.
const SMOOTH_DOWN: f32 = 0.4;
const SMOOTH_UP: f32 = 0.8;
/// Frames per block of the minimum tracker, and blocks remembered: the
/// noise floor is the quietest point of the last 3-4 blocks (1.5-2 s).
const BLOCK_FRAMES: u32 = 25;
const BLOCKS: usize = 3;
/// The minimum of a noisy spectrum sits below its mean; this lifts it back.
const NOISE_BIAS: f32 = 4.0;
/// Decision-directed a-priori SNR smoothing (Ephraim-Malah).
const DD_ALPHA: f32 = 0.98;
/// The most a frequency is turned down: 0.12 is about -18 dB.
const GAIN_FLOOR: f32 = 0.12;
/// Bins below 75 Hz (25 Hz each), always turned down: DC and rumble.
const RUMBLE_BINS: usize = 3;

pub struct Denoiser {
    cos: Vec<f32>,
    sin: Vec<f32>,
    window: Vec<f32>,
    input: Vec<f32>,
    overlap: Vec<f32>,
    buf: Vec<f32>,
    re: Vec<f32>,
    im: Vec<f32>,
    gain: Vec<f32>,
    smoothed: Vec<f32>,
    block_min: Vec<f32>,
    history: Vec<Vec<f32>>,
    slot: usize,
    clean: Vec<f32>,
    frames: u32,
}

impl Default for Denoiser {
    fn default() -> Self {
        Self::new()
    }
}

impl Denoiser {
    pub fn new() -> Self {
        let step = std::f64::consts::TAU / WIN as f64;
        Self {
            cos: (0..WIN).map(|i| (step * i as f64).cos() as f32).collect(),
            sin: (0..WIN).map(|i| (step * i as f64).sin() as f32).collect(),
            // sqrt of a periodic Hann window: analysis times synthesis is
            // Hann, which sums to exactly 1 at 50% overlap.
            window: (0..WIN).map(|i| (std::f64::consts::PI * i as f64 / WIN as f64).sin() as f32).collect(),
            input: vec![0.0; WIN],
            overlap: vec![0.0; HOP],
            buf: vec![0.0; WIN],
            re: vec![0.0; BINS],
            im: vec![0.0; BINS],
            gain: vec![1.0; BINS],
            smoothed: vec![0.0; BINS],
            block_min: vec![f32::INFINITY; BINS],
            history: vec![vec![f32::INFINITY; BINS]; BLOCKS],
            slot: 0,
            clean: vec![0.0; BINS],
            frames: 0,
        }
    }

    /// Forgets the noise it learned and the audio it holds.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Denoises one 20 ms frame in place. The frame that comes out is the
    /// one that went in a call earlier.
    pub fn process(&mut self, frame: &mut [i16]) {
        if frame.len() != HOP {
            return;
        }
        self.input.copy_within(HOP.., 0);
        for (d, s) in self.input[HOP..].iter_mut().zip(frame.iter()) {
            *d = *s as f32 / 32768.0;
        }
        for n in 0..WIN {
            self.buf[n] = self.input[n] * self.window[n];
        }
        self.forward();
        self.update_gains();
        for k in 0..BINS {
            let g = if k < RUMBLE_BINS {
                GAIN_FLOOR
            } else {
                // A little smoothing across frequency against "musical noise".
                let lo = self.gain[k - 1];
                let hi = self.gain[(k + 1).min(BINS - 1)];
                0.25 * lo + 0.5 * self.gain[k] + 0.25 * hi
            };
            self.re[k] *= g;
            self.im[k] *= g;
        }
        self.inverse();
        for n in 0..HOP {
            let v = self.overlap[n] + self.buf[n] * self.window[n];
            frame[n] = (v * 32768.0).clamp(-32768.0, 32767.0) as i16;
        }
        for n in 0..HOP {
            self.overlap[n] = self.buf[HOP + n] * self.window[HOP + n];
        }
        self.frames = self.frames.wrapping_add(1);
    }

    /// The real DFT of `buf` into `re`/`im`, bins 0..=WIN/2.
    fn forward(&mut self) {
        for k in 0..BINS {
            let (mut r, mut i, mut idx) = (0.0f32, 0.0f32, 0usize);
            for n in 0..WIN {
                let x = self.buf[n];
                r += x * self.cos[idx];
                i -= x * self.sin[idx];
                idx += k;
                if idx >= WIN {
                    idx -= WIN;
                }
            }
            self.re[k] = r;
            self.im[k] = i;
        }
    }

    /// The inverse of `forward`, from `re`/`im` back into `buf`.
    fn inverse(&mut self) {
        let scale = 1.0 / WIN as f32;
        let nyquist = self.re[BINS - 1];
        for n in 0..WIN {
            let mut acc = self.re[0] + if n % 2 == 0 { nyquist } else { -nyquist };
            let mut idx = n;
            for k in 1..BINS - 1 {
                acc += 2.0 * (self.re[k] * self.cos[idx] - self.im[k] * self.sin[idx]);
                idx += n;
                if idx >= WIN {
                    idx -= WIN;
                }
            }
            self.buf[n] = acc * scale;
        }
    }

    /// Tracks the noise floor and works out each bin's gain.
    fn update_gains(&mut self) {
        let first = self.frames == 0;
        for k in 0..BINS {
            let p = self.re[k] * self.re[k] + self.im[k] * self.im[k];
            let a = if p < self.smoothed[k] { SMOOTH_DOWN } else { SMOOTH_UP };
            let s = if first { p } else { a * self.smoothed[k] + (1.0 - a) * p };
            self.smoothed[k] = s;
            if s < self.block_min[k] {
                self.block_min[k] = s;
            }
            let floor = self.history.iter().fold(self.block_min[k], |m, h| m.min(h[k]));
            let noise = (floor * NOISE_BIAS).max(1e-12);
            let post = p / noise;
            let prio = DD_ALPHA * self.clean[k] / noise + (1.0 - DD_ALPHA) * (post - 1.0).max(0.0);
            let g = (prio / (1.0 + prio)).clamp(GAIN_FLOOR, 1.0);
            self.gain[k] = g;
            self.clean[k] = g * g * p;
        }
        if self.frames % BLOCK_FRAMES == BLOCK_FRAMES - 1 {
            self.history[self.slot].copy_from_slice(&self.block_min);
            self.slot = (self.slot + 1) % BLOCKS;
            self.block_min.fill(f32::INFINITY);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Denoiser, HOP};

    struct Noise(u64);
    impl Noise {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 33) as f32 / (1u64 << 31) as f32) * 2.0 - 1.0
        }
    }

    fn rms(x: &[i16]) -> f64 {
        (x.iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
    }

    #[test]
    fn silence_stays_silent() {
        let mut d = Denoiser::new();
        for _ in 0..50 {
            let mut f = vec![0i16; HOP];
            d.process(&mut f);
            assert!(f.iter().all(|&s| s == 0));
        }
    }

    #[test]
    fn steady_noise_is_turned_down() {
        let mut d = Denoiser::new();
        let mut rng = Noise(7);
        let (mut before, mut after) = (Vec::new(), Vec::new());
        for i in 0..250 {
            let mut f: Vec<i16> = (0..HOP).map(|_| (rng.next() * 2000.0) as i16).collect();
            if i >= 150 {
                before.extend_from_slice(&f);
            }
            d.process(&mut f);
            if i >= 150 {
                after.extend_from_slice(&f);
            }
        }
        assert!(rms(&after) < rms(&before) * 0.35, "{} vs {}", rms(&after), rms(&before));
    }

    #[test]
    fn speech_like_bursts_get_through() {
        let mut d = Denoiser::new();
        let mut rng = Noise(11);
        let (mut tone, mut out) = (Vec::new(), Vec::new());
        let mut t = 0usize;
        for i in 0..250 {
            let on = (i / 10) % 2 == 0;
            let mut clean = vec![0i16; HOP];
            let mut f = vec![0i16; HOP];
            for n in 0..HOP {
                let s = if on { (std::f32::consts::TAU * 440.0 * t as f32 / 16_000.0).sin() * 8000.0 } else { 0.0 };
                t += 1;
                clean[n] = s as i16;
                f[n] = (s + rng.next() * 200.0) as i16;
            }
            d.process(&mut f);
            if i >= 150 {
                tone.extend_from_slice(&clean);
                out.extend_from_slice(&f);
            }
        }
        assert!(rms(&out) > rms(&tone) * 0.7, "{} vs {}", rms(&out), rms(&tone));
    }
}
