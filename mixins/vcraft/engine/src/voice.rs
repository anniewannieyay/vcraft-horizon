//! The voice engine (`VoiceEngine`): the microphone at 16 kHz mono in 20 ms
//! frames, voice activity or push to talk, IMA ADPCM, encrypted to the relay;
//! and a jitter-buffered mixer of everyone else's frames to the speakers.
//! cpal's streams run at the device's own rate; this resamples both ways.

use crate::adpcm;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SizedSample, Stream, StreamConfig};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

pub const SAMPLE_RATE: u32 = 16_000;
pub const FRAME_SAMPLES: usize = 320;
const JITTER_FRAMES: usize = 3;
const MAX_QUEUE: usize = 12;
const VOICE_HANGOVER: Duration = Duration::from_millis(400);

/// What the game thread tells the voice threads, refreshed every tick.
#[derive(Clone, Default)]
pub struct Params {
    pub muted: bool,
    pub deafened: bool,
    pub active: bool,
    pub in_call: bool,
    pub me: Option<String>,
    pub push_to_talk: bool,
    pub push_to_talk_held: bool,
    pub threshold_db: f64,
    pub mic_volume: f64,
    pub output_volume: f64,
    pub muted_users: HashSet<String>,
    /// Proximity Chat's volume per player in a room (absent: full volume).
    pub room_gains: Option<HashMap<String, f64>>,
}

#[derive(Default)]
struct Speaker {
    queue: VecDeque<Vec<i16>>,
    primed: bool,
    last_frame: Option<Instant>,
}

pub struct Voice {
    running: AtomicBool,
    generation: AtomicU64,
    mic_test: AtomicBool,
    pub params: Mutex<Params>,
    speakers: Mutex<HashMap<String, Speaker>>,
    mic_level_db: Mutex<f32>,
    error: Mutex<Option<String>>,
    pub last_spoke: Mutex<HashMap<String, Instant>>,
}

struct MicQueue {
    samples: Mutex<VecDeque<i16>>,
    ready: Condvar,
}

impl Voice {
    pub fn new() -> Self {
        Self {
            running: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            mic_test: AtomicBool::new(false),
            params: Mutex::default(),
            speakers: Mutex::default(),
            mic_level_db: Mutex::new(-100.0),
            error: Mutex::new(None),
            last_spoke: Mutex::default(),
        }
    }

    pub fn running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
    pub fn mic_test(&self) -> bool {
        self.mic_test.load(Ordering::SeqCst)
    }
    pub fn mic_level_db(&self) -> f32 {
        *self.mic_level_db.lock().unwrap()
    }
    pub fn error(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }

    pub fn is_speaking(&self, uuid: &str) -> bool {
        self.last_spoke.lock().unwrap().get(uuid).is_some_and(|t| t.elapsed() < Duration::from_millis(300))
    }

    pub fn start(self: &Arc<Self>) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        *self.error.lock().unwrap() = None;
        let run_id = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let me = self.clone();
        let _ = std::thread::Builder::new().name("VCraft Audio".into()).spawn(move || me.audio_thread(run_id));
    }

    pub fn stop(&self) {
        if !self.running() {
            return;
        }
        let active = self.params.lock().unwrap().active;
        if self.mic_test() && !active {
            return;
        }
        self.running.store(false, Ordering::SeqCst);
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.speakers.lock().unwrap().clear();
        *self.mic_level_db.lock().unwrap() = -100.0;
    }

    pub fn set_mic_test(self: &Arc<Self>, on: bool) {
        self.mic_test.store(on, Ordering::SeqCst);
        if on {
            self.start();
        } else if !self.params.lock().unwrap().active {
            self.stop();
        }
    }

    fn alive(&self, run_id: u64) -> bool {
        self.running() && self.generation.load(Ordering::SeqCst) == run_id
    }

    /// A frame from the relay: sender UUID, then the encrypted ADPCM frame.
    pub fn receive(&self, data: &[u8]) {
        if !self.running() || data.len() < 19 {
            return;
        }
        let p = self.params.lock().unwrap().clone();
        if p.deafened {
            return;
        }
        let uuid = crate::util::uuid_string(u128::from_be_bytes(data[..16].try_into().unwrap()));
        if p.muted_users.contains(&uuid) {
            return;
        }
        let Some(frame) = crate::with_secure(|s| s.decrypt_voice(p.in_call, &uuid, &data[16..])) else {
            return;
        };
        let mut pcm = adpcm::decode(&frame);
        if !p.in_call {
            if let Some(gains) = &p.room_gains {
                let gain = gains.get(&uuid).copied().unwrap_or(0.0).clamp(0.0, 1.0);
                if gain < 0.01 {
                    return;
                }
                if gain < 0.999 {
                    for s in &mut pcm {
                        *s = (*s as f64 * gain) as i16;
                    }
                }
            }
        }
        self.enqueue(&uuid, pcm);
    }

    fn enqueue(&self, uuid: &str, pcm: Vec<i16>) {
        {
            let mut speakers = self.speakers.lock().unwrap();
            let s = speakers.entry(uuid.to_owned()).or_default();
            if s.queue.len() >= MAX_QUEUE {
                s.queue.pop_front();
            }
            s.queue.push_back(pcm);
            s.last_frame = Some(Instant::now());
        }
        self.last_spoke.lock().unwrap().insert(uuid.to_owned(), Instant::now());
    }

    /// One mixed 20 ms frame for the speakers.
    fn mix(&self) -> [i16; FRAME_SAMPLES] {
        let mut mix = [0i32; FRAME_SAMPLES];
        {
            let mut speakers = self.speakers.lock().unwrap();
            speakers.retain(|_, s| !(s.queue.is_empty() && s.last_frame.is_some_and(|t| t.elapsed() > Duration::from_secs(5))));
            for s in speakers.values_mut() {
                if !s.primed && s.queue.len() >= JITTER_FRAMES {
                    s.primed = true;
                }
                if s.primed {
                    match s.queue.pop_front() {
                        Some(frame) => {
                            for (m, v) in mix.iter_mut().zip(frame.iter()) {
                                *m += *v as i32;
                            }
                        }
                        None => s.primed = false,
                    }
                }
            }
        }
        let p = self.params.lock().unwrap();
        let volume = if p.deafened { 0.0 } else { p.output_volume };
        std::array::from_fn(|i| (mix[i] as f64 * volume).clamp(-32768.0, 32767.0) as i16)
    }

    fn audio_thread(self: Arc<Self>, run_id: u64) {
        let host = cpal::default_host();
        let mic = Arc::new(MicQueue { samples: Mutex::new(VecDeque::new()), ready: Condvar::new() });
        let output = match open_output(&host, self.clone()) {
            Ok(s) => Some(s),
            Err(e) => {
                *self.error.lock().unwrap() = Some(format!("Speaker unavailable: {e}"));
                None
            }
        };
        let input = match open_input(&host, mic.clone()) {
            Ok(s) => Some(s),
            Err(e) => {
                *self.error.lock().unwrap() = Some(format!("Microphone unavailable: {e}"));
                None
            }
        };
        if input.is_some() {
            let me = self.clone();
            let mic2 = mic.clone();
            let _ = std::thread::Builder::new().name("VCraft Mic".into()).spawn(move || me.capture_loop(run_id, &mic2));
        }
        while self.alive(run_id) {
            std::thread::sleep(Duration::from_millis(50));
        }
        mic.ready.notify_all();
        drop(input);
        drop(output);
    }

    fn capture_loop(&self, run_id: u64, mic: &MicQueue) {
        let mut encoder = adpcm::Encoder::default();
        let mut hangover_until = Instant::now();
        while self.alive(run_id) {
            let frame: Vec<i16> = {
                let mut q = mic.samples.lock().unwrap();
                while q.len() < FRAME_SAMPLES && self.alive(run_id) {
                    q = mic.ready.wait_timeout(q, Duration::from_millis(100)).unwrap().0;
                }
                if q.len() < FRAME_SAMPLES {
                    continue;
                }
                q.drain(..FRAME_SAMPLES).collect()
            };
            let p = self.params.lock().unwrap().clone();
            let mut sum = 0.0f64;
            let pcm: Vec<i16> = frame
                .iter()
                .map(|s| {
                    let v = (*s as f64 * p.mic_volume).clamp(-32768.0, 32767.0);
                    sum += v * v;
                    v as i16
                })
                .collect();
            let rms = (sum / FRAME_SAMPLES as f64).sqrt() / 32768.0;
            let db = if rms <= 0.0 { -100.0 } else { 20.0 * rms.log10() } as f32;
            *self.mic_level_db.lock().unwrap() = db;
            let now = Instant::now();
            let talking = if p.push_to_talk {
                p.push_to_talk_held
            } else {
                if db as f64 >= p.threshold_db {
                    hangover_until = now + VOICE_HANGOVER;
                }
                now < hangover_until
            };
            if !talking {
                continue;
            }
            if self.mic_test() && !p.active {
                self.enqueue("self-test", pcm);
            } else if !p.muted && !p.deafened && p.active {
                let Some(me) = p.me.clone() else { continue };
                let encoded = encoder.encode(&pcm);
                if let Some(sealed) = crate::with_secure(|s| s.encrypt_voice(p.in_call, &encoded, &me)) {
                    crate::relay().send_voice(sealed);
                    self.last_spoke.lock().unwrap().insert(me, now);
                }
            }
        }
    }
}

/// Linear resampling from one rate to another, a sample at a time.
struct Resampler {
    step: f64,
    phase: f64,
    last: f32,
}

impl Resampler {
    fn new(from: u32, to: u32) -> Self {
        Self { step: from as f64 / to as f64, phase: 0.0, last: 0.0 }
    }
    /// Feeds one input sample, pushing the output samples it completes.
    fn push(&mut self, sample: f32, out: &mut impl FnMut(f32)) {
        while self.phase < 1.0 {
            out(self.last + (sample - self.last) * self.phase as f32);
            self.phase += self.step;
        }
        self.phase -= 1.0;
        self.last = sample;
    }
}

fn usable(f: SampleFormat) -> bool {
    matches!(f, SampleFormat::F32 | SampleFormat::I16 | SampleFormat::I32 | SampleFormat::U16)
}

fn pick(configs: Vec<cpal::SupportedStreamConfigRange>, default: Option<cpal::SupportedStreamConfig>) -> Option<cpal::SupportedStreamConfig> {
    if let Some(d) = default.filter(|d| usable(d.sample_format())) {
        return Some(d);
    }
    let mut c: Vec<_> = configs.into_iter().filter(|c| usable(c.sample_format())).collect();
    c.sort_by_key(|c| (c.channels(), c.sample_format() != SampleFormat::F32));
    c.into_iter().next().map(|c| c.with_max_sample_rate())
}

fn open_output(host: &cpal::Host, voice: Arc<Voice>) -> Result<Stream, String> {
    let device = host.default_output_device().ok_or("no speakers")?;
    let configs = device.supported_output_configs().map(|c| c.collect()).unwrap_or_default();
    let supported = pick(configs, device.default_output_config().ok()).ok_or("no usable speaker format")?;
    let config: StreamConfig = supported.config();
    let stream = match supported.sample_format() {
        SampleFormat::F32 => output::<f32>(&device, &config, voice, |v| v),
        SampleFormat::I16 => output::<i16>(&device, &config, voice, |v| (v * 32767.0) as i16),
        SampleFormat::I32 => output::<i32>(&device, &config, voice, |v| (v as f64 * 2147483647.0) as i32),
        SampleFormat::U16 => output::<u16>(&device, &config, voice, |v| ((v + 1.0) * 32767.5) as u16),
        _ => return Err("no usable speaker format".into()),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}

fn output<T: SizedSample + Send + 'static>(
    device: &cpal::Device,
    config: &StreamConfig,
    voice: Arc<Voice>,
    convert: fn(f32) -> T,
) -> Result<Stream, String> {
    let channels = config.channels as usize;
    let mut resampler = Resampler::new(SAMPLE_RATE, config.sample_rate.0);
    let mut ready: VecDeque<f32> = VecDeque::new();
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                let frames = data.len() / channels.max(1);
                while ready.len() < frames {
                    let mixed = voice.mix();
                    for s in mixed {
                        resampler.push(s as f32 / 32768.0, &mut |v| ready.push_back(v));
                    }
                }
                for frame in data.chunks_mut(channels) {
                    let v = convert(ready.pop_front().unwrap_or(0.0));
                    for s in frame.iter_mut() {
                        *s = v;
                    }
                }
            },
            |e| eprintln!("VCraft: speaker error: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

fn open_input(host: &cpal::Host, mic: Arc<MicQueue>) -> Result<Stream, String> {
    let device = host.default_input_device().ok_or("no microphone")?;
    let configs = device.supported_input_configs().map(|c| c.collect()).unwrap_or_default();
    let supported = pick(configs, device.default_input_config().ok()).ok_or("no usable microphone format")?;
    let config: StreamConfig = supported.config();
    let stream = match supported.sample_format() {
        SampleFormat::F32 => input::<f32>(&device, &config, mic, |v| v),
        SampleFormat::I16 => input::<i16>(&device, &config, mic, |v| v as f32 / 32768.0),
        SampleFormat::I32 => input::<i32>(&device, &config, mic, |v| (v as f64 / 2147483648.0) as f32),
        SampleFormat::U16 => input::<u16>(&device, &config, mic, |v| v as f32 / 32768.0 - 1.0),
        _ => return Err("no usable microphone format".into()),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}

fn input<T: SizedSample + Send + 'static>(
    device: &cpal::Device,
    config: &StreamConfig,
    mic: Arc<MicQueue>,
    convert: fn(T) -> f32,
) -> Result<Stream, String> {
    let channels = config.channels as usize;
    let mut resampler = Resampler::new(config.sample_rate.0, SAMPLE_RATE);
    let mut mono: Vec<i16> = Vec::new();
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                mono.clear();
                for frame in data.chunks(channels.max(1)) {
                    let sum: f32 = frame.iter().map(|s| convert(*s)).sum();
                    resampler.push(sum / frame.len() as f32, &mut |v| mono.push((v.clamp(-1.0, 1.0) * 32767.0) as i16));
                }
                let mut q = mic.samples.lock().unwrap();
                q.extend(mono.iter().copied());
                let excess = q.len().saturating_sub(SAMPLE_RATE as usize);
                q.drain(..excess);
                drop(q);
                mic.ready.notify_one();
            },
            |e| eprintln!("VCraft: microphone error: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn resampler_keeps_the_rate() {
        let mut r = super::Resampler::new(48_000, 16_000);
        let mut n = 0;
        for _ in 0..48_000 {
            r.push(0.5, &mut |_| n += 1);
        }
        assert!((15_990..=16_010).contains(&n), "{n}");
        let mut up = super::Resampler::new(16_000, 44_100);
        let mut m = 0;
        for _ in 0..16_000 {
            up.push(0.0, &mut |_| m += 1);
        }
        assert!((44_090..=44_110).contains(&m), "{m}");
    }
}
