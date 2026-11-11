//! IMA ADPCM, VCraft's voice codec: a 3-byte header (predictor, step index)
//! then two 4-bit samples a byte, low nibble first.

const INDEX_TABLE: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];
const STEP_TABLE: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66, 73, 80, 88, 97, 107, 118, 130,
    143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449, 494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166,
    1282, 1411, 1552, 1707, 1878, 2066, 2272, 2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845,
    8630, 9493, 10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];

fn clamp16(v: i32) -> i32 {
    v.clamp(-32768, 32767)
}

fn clamp_index(i: i32) -> i32 {
    i.clamp(0, 88)
}

pub fn decode(data: &[u8]) -> Vec<i16> {
    if data.len() < 3 {
        return Vec::new();
    }
    let mut predictor = i16::from_be_bytes([data[0], data[1]]) as i32;
    let mut index = clamp_index(data[2] as i32);
    let mut out = Vec::with_capacity((data.len() - 3) * 2);
    for &b in &data[3..] {
        for half in 0..2 {
            let nibble = if half == 0 { b & 15 } else { b >> 4 } as i32;
            let step = STEP_TABLE[index as usize];
            let mut delta = step >> 3;
            if nibble & 4 != 0 {
                delta += step;
            }
            if nibble & 2 != 0 {
                delta += step >> 1;
            }
            if nibble & 1 != 0 {
                delta += step >> 2;
            }
            predictor = clamp16(if nibble & 8 != 0 { predictor - delta } else { predictor + delta });
            index = clamp_index(index + INDEX_TABLE[nibble as usize]);
            out.push(predictor as i16);
        }
    }
    out
}

#[derive(Default)]
pub struct Encoder {
    predictor: i32,
    index: i32,
}

impl Encoder {
    pub fn encode(&mut self, pcm: &[i16]) -> Vec<u8> {
        let mut out = Vec::with_capacity(3 + pcm.len() / 2);
        out.push((self.predictor >> 8) as u8);
        out.push(self.predictor as u8);
        out.push(self.index as u8);
        for pair in pcm.chunks(2) {
            let lo = self.sample(pair[0] as i32);
            let hi = self.sample(*pair.get(1).unwrap_or(&0) as i32);
            out.push((lo | hi << 4) as u8);
        }
        out
    }

    fn sample(&mut self, sample: i32) -> i32 {
        let mut step = STEP_TABLE[self.index as usize];
        let mut diff = sample - self.predictor;
        let mut nibble = 0;
        if diff < 0 {
            nibble = 8;
            diff = -diff;
        }
        let mut delta = step >> 3;
        if diff >= step {
            nibble |= 4;
            diff -= step;
            delta += step;
        }
        step >>= 1;
        if diff >= step {
            nibble |= 2;
            diff -= step;
            delta += step;
        }
        step >>= 1;
        if diff >= step {
            nibble |= 1;
            delta += step;
        }
        self.predictor = clamp16(if nibble & 8 != 0 { self.predictor - delta } else { self.predictor + delta });
        self.index = clamp_index(self.index + INDEX_TABLE[nibble as usize]);
        nibble
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn round_trip_tracks_a_sine() {
        let pcm: Vec<i16> = (0..320).map(|i| ((i as f32 * 0.1).sin() * 8000.0) as i16).collect();
        let mut enc = super::Encoder::default();
        let first = enc.encode(&pcm);
        let second = enc.encode(&pcm);
        assert_eq!(first.len(), 3 + 160);
        let out = super::decode(&second);
        assert_eq!(out.len(), 320);
        let err: i64 = pcm.iter().zip(&out).map(|(a, b)| (*a as i64 - *b as i64).abs()).sum::<i64>() / 320;
        assert!(err < 600, "average error {err}");
    }
}
