pub const NUM_SAMPLES: usize = 32768;
pub const NUM_CHANNELS: usize = 3;
pub const SAMPLE_RATE: f32 = 20000.0;

const PHASE_OFFSETS: [f32; NUM_CHANNELS] = [
    0.0,
    2.0 * std::f32::consts::PI / 3.0,
    4.0 * std::f32::consts::PI / 3.0,
];

pub struct MotorSimulator {
    /// Interleaved buffer: [ph1_0, ph2_0, ph3_0, ph1_1, ph2_1, ph3_1, ...]
    pub buffer: [f32; NUM_SAMPLES * NUM_CHANNELS],
    pub write_pos: u32,
    sample_index: u64,
    sample_rate: f32,
    rng_state: u64,
}

impl MotorSimulator {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            buffer: [0.0; NUM_SAMPLES * NUM_CHANNELS],
            write_pos: 0,
            sample_index: 0,
            sample_rate,
            rng_state: 0xDEAD_BEEF_CAFE_BABEu64,
        }
    }

    pub fn generate_samples(&mut self, count: usize, amplitude: f32, frequency: f32) {
        for _ in 0..count {
            let t = self.sample_index as f32 / self.sample_rate;
            let base_angle = 2.0 * std::f32::consts::PI * frequency * t;

            let base_idx = self.write_pos as usize * NUM_CHANNELS;
            for (ch, &offset) in PHASE_OFFSETS.iter().enumerate() {
                let phase_current = amplitude * (base_angle + offset).sin();
                let noise = self.random_normal() * 0.05 * amplitude;
                self.buffer[base_idx + ch] = phase_current + noise;
            }

            self.write_pos = (self.write_pos + 1) % NUM_SAMPLES as u32;
            self.sample_index += 1;
        }
    }

    fn random_u32(&mut self) -> u32 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        (self.rng_state & 0xFFFF_FFFF) as u32
    }

    fn random_normal(&mut self) -> f32 {
        let mut sum = 0.0f32;
        for _ in 0..6 {
            sum += (self.random_u32() as f32) / (u32::MAX as f32);
        }
        sum - 3.0
    }
}
