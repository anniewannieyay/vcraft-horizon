//! Echo cancellation for people on speakers: what VCraft plays is picked up
//! again by the microphone. An NLMS adaptive filter learns the path from the
//! speakers to the microphone (after finding the delay between them from
//! the two signals' loudness), subtracts the echo it predicts, and a
//! suppressor turns down what is left while only the far end talks.
//!
//! Everything runs at 16 kHz on the microphone thread, a 20 ms frame at a time.

use std::collections::VecDeque;
use std::sync::Mutex;

/// Taps of the adaptive filter: 32 ms of room around the delay.
const TAPS: usize = 512;
/// How far before the estimated delay the filter starts.
const LEAD: usize = 128;
/// The longest speaker-to-microphone delay looked for: 500 ms.
const MAX_DELAY: usize = 8000;
/// Loudness blocks for the delay search: 10 ms.
const BLOCK: usize = 160;
/// Blocks of history the delay search correlates: 4 s.
const SEARCH_BLOCKS: usize = 400;
const MU: f32 = 0.35;

/// What the speakers played, at 16 kHz: the mixer writes, the microphone reads.
#[derive(Default)]
pub struct FarEnd {
    inner: Mutex<FarInner>,
}

#[derive(Default)]
struct FarInner {
    samples: VecDeque<f32>,
    /// Samples written since the start, including those dropped.
    total: u64,
}

const FAR_KEEP: usize = 32_000;

impl FarEnd {
    pub fn push(&self, frame: &[f32]) {
        let mut f = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        f.samples.extend(frame.iter().copied());
        f.total += frame.len() as u64;
        let excess = f.samples.len().saturating_sub(FAR_KEEP);
        f.samples.drain(..excess);
    }

    /// The samples written after `since` (an earlier `total`), and the new total.
    pub fn since(&self, since: u64) -> (Vec<f32>, u64) {
        let f = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let new = (f.total.saturating_sub(since) as usize).min(f.samples.len());
        let start = f.samples.len() - new;
        (f.samples.iter().skip(start).copied().collect(), f.total)
    }

    pub fn total(&self) -> u64 {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).total
    }
}

pub struct Aec {
    /// Far-end history; `hist[i]` is far sample `base + i`.
    hist: VecDeque<f32>,
    base: u64,
    read: u64,
    w: Vec<f32>,
    delay: usize,
    /// Loudness of each far block, by block number (`far sample / BLOCK`).
    far_env: VecDeque<f32>,
    far_env_base: u64,
    far_acc: (f32, usize),
    /// Microphone blocks: (loudness, the far sample count when it was processed).
    mic_env: VecDeque<(f32, u64)>,
    frames: u64,
    gain: f32,
    pub converged: bool,
}

impl Default for Aec {
    fn default() -> Self {
        Self::new()
    }
}

impl Aec {
    pub fn new() -> Self {
        Self {
            hist: VecDeque::new(),
            base: 0,
            read: 0,
            w: vec![0.0; TAPS],
            delay: 0,
            far_env: VecDeque::new(),
            far_env_base: 0,
            far_acc: (0.0, 0),
            mic_env: VecDeque::new(),
            frames: 0,
            gain: 1.0,
            converged: false,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn delay_ms(&self) -> u32 {
        (self.delay * 1000 / 16_000) as u32
    }

    /// Takes the far-end samples played since the last frame.
    pub fn feed_far(&mut self, far: &FarEnd) {
        if self.read == 0 && self.hist.is_empty() {
            let t = far.total();
            self.read = t;
            self.base = t;
            self.far_env_base = t / BLOCK as u64;
            self.far_acc = (0.0, (t % BLOCK as u64) as usize);
        }
        let (new, total) = far.since(self.read);
        let skipped = total - self.read - new.len() as u64;
        if skipped > 0 {
            // The microphone fell behind by more than the far end keeps.
            let t = total - new.len() as u64;
            self.hist.clear();
            self.base = t;
            self.far_env.clear();
            self.far_env_base = t / BLOCK as u64;
            self.far_acc = (0.0, (t % BLOCK as u64) as usize);
            self.mic_env.clear();
        }
        self.read = total;
        for s in new {
            self.hist.push_back(s);
            self.far_acc.0 += s * s;
            self.far_acc.1 += 1;
            if self.far_acc.1 == BLOCK {
                self.far_env.push_back((self.far_acc.0 / BLOCK as f32).sqrt());
                self.far_acc = (0.0, 0);
            }
        }
        let keep = MAX_DELAY + TAPS + 4 * crate::voice::FRAME_SAMPLES;
        let excess = self.hist.len().saturating_sub(keep);
        self.hist.drain(..excess);
        self.base += excess as u64;
        let env_excess = self.far_env.len().saturating_sub(SEARCH_BLOCKS + MAX_DELAY / BLOCK + 8);
        self.far_env.drain(..env_excess);
        self.far_env_base += env_excess as u64;
    }

    fn far_at(&self, index: i64) -> f32 {
        let i = index - self.base as i64;
        if i < 0 {
            return 0.0;
        }
        self.hist.get(i as usize).copied().unwrap_or(0.0)
    }

    /// Cancels the echo in one microphone frame (samples in -1..1).
    pub fn process(&mut self, mic: &mut [f32]) {
        let n = mic.len();
        let now = self.read as i64;
        // Loudness of this frame's blocks, for the delay search.
        for (c, chunk) in mic.chunks(BLOCK).enumerate() {
            let e = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
            let at = (self.read + ((c + 1) * BLOCK) as u64).saturating_sub(n as u64);
            self.mic_env.push_back((e, at));
        }
        let excess = self.mic_env.len().saturating_sub(SEARCH_BLOCKS);
        self.mic_env.drain(..excess);
        self.frames += 1;
        if self.frames % 25 == 0 {
            self.estimate_delay();
        }
        let start = self.delay.saturating_sub(LEAD) as i64;
        let (mut e_mic, mut e_err, mut e_far, mut e_hat) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let mut x = vec![0.0f32; TAPS];
        for (k, d) in mic.iter_mut().enumerate() {
            // The far sample heard "now" for this microphone sample.
            let t = now - n as i64 + k as i64 - start;
            let mut power = 1e-6f32;
            for (j, xj) in x.iter_mut().enumerate() {
                let v = self.far_at(t - j as i64);
                *xj = v;
                power += v * v;
            }
            let y: f32 = self.w.iter().zip(&x).map(|(w, x)| w * x).sum();
            let err = *d - y;
            e_mic += *d * *d;
            e_err += err * err;
            e_hat += y * y;
            e_far += x[0] * x[0];
            // Adapt only while the far end talks and the near end mostly doesn't.
            let far_active = power / TAPS as f32 > 1e-6;
            let double_talk = *d * *d > 4.0 * (y * y) + 1e-4 && self.converged;
            if far_active && !double_talk {
                let step = MU * err / power;
                for (w, xj) in self.w.iter_mut().zip(&x) {
                    *w += step * xj;
                }
            }
            *d = err;
        }
        let nf = n as f32;
        let (e_mic, e_err, e_far, e_hat) = (e_mic / nf, e_err / nf, e_far / nf, e_hat / nf);
        if e_far > 1e-5 && e_err < e_mic * 0.25 {
            self.converged = true;
        }
        // Residual suppression: while the far end talks and what's left
        // is mostly echo, turn the frame down.
        let far_talks = e_far > 1e-5;
        let near_talks = e_err > 2.0 * e_hat.max(1e-7) && e_err > 1e-5;
        let target = if far_talks && !near_talks { 0.15 } else { 1.0 };
        for s in mic.iter_mut() {
            self.gain += (target - self.gain) * if target < self.gain { 0.02 } else { 0.004 };
            *s *= self.gain;
        }
    }

    /// Finds the delay where the far end's loudness best matches the microphone's.
    fn estimate_delay(&mut self) {
        if self.mic_env.len() < 100 {
            return;
        }
        let far_mean = self.far_env.iter().sum::<f32>() / self.far_env.len().max(1) as f32;
        let far_var = self.far_env.iter().map(|v| (v - far_mean).powi(2)).sum::<f32>();
        if far_var < 1e-6 {
            return;
        }
        let mic_mean = self.mic_env.iter().map(|m| m.0).sum::<f32>() / self.mic_env.len() as f32;
        let mut best = (0.0f32, 0usize);
        for lag in 0..=MAX_DELAY / BLOCK {
            let (mut num, mut fv, mut mv) = (0.0f32, 0.0f32, 0.0f32);
            for (m, at) in &self.mic_env {
                let block = (*at / BLOCK as u64) as i64 - lag as i64 - 1;
                let i = block - self.far_env_base as i64;
                if i < 0 || i as usize >= self.far_env.len() {
                    continue;
                }
                let f = self.far_env[i as usize] - far_mean;
                let mm = m - mic_mean;
                num += f * mm;
                fv += f * f;
                mv += mm * mm;
            }
            let corr = num / (fv * mv).sqrt().max(1e-9);
            if corr > best.0 {
                best = (corr, lag);
            }
        }
        if best.0 > 0.4 {
            let delay = best.1 * BLOCK;
            if delay.abs_diff(self.delay) > BLOCK {
                self.delay = delay;
                self.w.iter_mut().for_each(|w| *w = 0.0);
                self.converged = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(seed: &mut u32) -> f32 {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 17;
        *seed ^= *seed << 5;
        (*seed as f32 / u32::MAX as f32 - 0.5) * 0.6
    }

    /// Far-end noise, heard back 150 ms later at half volume through a
    /// small "room", is mostly removed after a few seconds.
    #[test]
    fn removes_a_delayed_echo() {
        let far = FarEnd::default();
        let mut aec = Aec::new();
        let mut seed = 12345u32;
        let mut played: Vec<f32> = Vec::new();
        let delay = 2400;
        let (mut before, mut after) = (0.0f32, 0.0f32);
        for frame in 0..400 {
            let out: Vec<f32> = (0..320).map(|_| noise(&mut seed)).collect();
            far.push(&out);
            played.extend(&out);
            let end = played.len();
            let mut mic: Vec<f32> = (end - 320..end)
                .map(|i| {
                    let a = if i >= delay { played[i - delay] } else { 0.0 };
                    let b = if i >= delay + 7 { played[i - delay - 7] } else { 0.0 };
                    0.5 * a + 0.2 * b
                })
                .collect();
            let e_in: f32 = mic.iter().map(|s| s * s).sum();
            aec.feed_far(&far);
            aec.process(&mut mic);
            let e_out: f32 = mic.iter().map(|s| s * s).sum();
            if frame >= 300 {
                before += e_in;
                after += e_out;
            }
        }
        let db = 10.0 * (after / before).log10();
        assert!(db < -15.0, "only {db:.1} dB of echo removed (delay found {} ms)", aec.delay_ms());
    }

    #[test]
    fn leaves_the_voice_alone_when_nothing_plays() {
        let far = FarEnd::default();
        let mut aec = Aec::new();
        let mut seed = 99u32;
        let mut kept = 0.0f32;
        let mut total = 0.0f32;
        for _ in 0..100 {
            far.push(&[0.0; 320]);
            let mut mic: Vec<f32> = (0..320).map(|_| noise(&mut seed)).collect();
            total += mic.iter().map(|s| s * s).sum::<f32>();
            aec.feed_far(&far);
            aec.process(&mut mic);
            kept += mic.iter().map(|s| s * s).sum::<f32>();
        }
        assert!(kept > total * 0.9);
    }
}
