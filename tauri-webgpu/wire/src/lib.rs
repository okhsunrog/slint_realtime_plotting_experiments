//! Shared versioned transport contract: three interleaved f32s per frame.
use serde::{Deserialize, Serialize};
pub const CHANNELS: usize = 3;
pub const HEADER: usize = 64;
pub const CAPACITY: usize = 1 << 20;
const MAGIC: u32 = 0x504c4f54;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub mode: String,
    pub rate: u32,
    pub batch: u32,
    pub frequency: f64,
    pub amplitude: f64,
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if !["json", "binary", "invoke", "ws", "protocol"].contains(&self.mode.as_str())
            || !(1_000..=8_000_000).contains(&self.rate)
            || !(64..=65_536).contains(&self.batch)
            || !self.frequency.is_finite()
            || !(0.1..=1000.).contains(&self.frequency)
            || !self.amplitude.is_finite()
            || !(0.1..=10.).contains(&self.amplitude)
        {
            return Err("Invalid stream configuration".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Batch {
    pub session: u32,
    pub seq: u32,
    pub first: u64,
    pub rate: u32,
    pub created_ms: f64,
    pub samples: Vec<f32>,
}
impl Batch {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![0u8; HEADER + self.samples.len() * 4];
        for (offset, value) in [
            (0, MAGIC),
            (4, 1),
            (8, self.session),
            (12, self.seq),
            (16, (self.samples.len() / CHANNELS) as u32),
            (20, CHANNELS as u32),
            (24, self.rate),
        ] {
            out[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        out[32..40].copy_from_slice(&self.first.to_le_bytes());
        out[40..48].copy_from_slice(&self.created_ms.to_le_bytes());
        for (bytes, value) in out[HEADER..].chunks_exact_mut(4).zip(&self.samples) {
            bytes.copy_from_slice(&value.to_le_bytes());
        }
        out
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() < HEADER {
            return Err("Truncated header");
        }
        let u32_at = |i| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
        if u32_at(0) != MAGIC || u32_at(4) != 1 || u32_at(20) != CHANNELS as u32 {
            return Err("Unsupported packet");
        }
        let frames = u32_at(16) as usize;
        if !(1..=65_536).contains(&frames) || bytes.len() != HEADER + frames * CHANNELS * 4 {
            return Err("Invalid payload length");
        }
        let samples = bytes[HEADER..]
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect::<Vec<_>>();
        if samples.iter().any(|x| !x.is_finite()) {
            return Err("Non-finite sample");
        }
        Ok(Self {
            session: u32_at(8),
            seq: u32_at(12),
            rate: u32_at(24),
            first: u64::from_le_bytes(bytes[32..40].try_into().unwrap()),
            created_ms: f64::from_le_bytes(bytes[40..48].try_into().unwrap()),
            samples,
        })
    }
}
/// Absolute sample indices make the signal independent of transport/batch size.
pub fn generate(config: &Config, first: u64) -> Vec<f32> {
    Generator::new(config.clone()).batch(first)
}
pub struct Generator {
    config: Config,
    table: Vec<[f32; CHANNELS]>,
}
impl Generator {
    pub fn new(config: Config) -> Self {
        let table = (0..8192)
            .map(|i| {
                std::array::from_fn(|c| {
                    let p = std::f64::consts::TAU * (i as f64 / 8192. + c as f64 / 3.);
                    ((p.sin() + 0.06 * (p * 11.).sin()) * config.amplitude) as f32
                })
            })
            .collect();
        Self { config, table }
    }
    pub fn batch(&self, first: u64) -> Vec<f32> {
        let config = &self.config;
        let mut samples = Vec::with_capacity(config.batch as usize * CHANNELS);
        for i in first..first + config.batch as u64 {
            let index = (i as f64 * config.frequency * 8192. / config.rate as f64) as usize % 8192;
            let spike = if i % 8191 < 3 {
                0.28 * config.amplitude as f32
            } else {
                0.
            };
            for &v in &self.table[index] {
                samples.push(v + spike);
            }
        }
        samples
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Config {
        Config {
            mode: "binary".into(),
            rate: 200_000,
            batch: 64,
            frequency: 7.,
            amplitude: 1.,
        }
    }
    #[test]
    fn binary_roundtrip_and_reject_corruption() {
        let b = Batch {
            session: 9,
            seq: 17,
            first: 8190,
            rate: 200_000,
            created_ms: 123.25,
            samples: generate(&config(), 8190),
        };
        let mut bytes = b.encode();
        let decoded = Batch::decode(&bytes).unwrap();
        assert_eq!(decoded.samples, b.samples);
        assert_eq!(decoded.first, b.first);
        assert_eq!(decoded.created_ms, b.created_ms);
        assert_eq!(decoded.seq, 17);
        assert!(Batch::decode(&bytes[..bytes.len() - 1]).is_err());
        bytes[4] = 2;
        assert!(Batch::decode(&bytes).is_err());
    }
    #[test]
    fn source_is_independent_of_batch_boundaries() {
        let mut c = config();
        let a = generate(&c, 0);
        let b = generate(&c, 64);
        c.batch = 128;
        assert_eq!([a, b].concat(), generate(&c, 0));
        c.frequency = f64::NAN;
        assert!(c.validate().is_err());
    }
}
