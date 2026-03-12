pub const NUM_SAMPLES: usize = 1024;

pub struct AdcSimulator {
    pub buffer: [f32; NUM_SAMPLES],
    pub write_pos: u32,
    sample_index: u64,
    sample_rate: f32,
    rng_state: u64,
}

impl AdcSimulator {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            buffer: [0.0; NUM_SAMPLES],
            write_pos: 0,
            sample_index: 0,
            sample_rate,
            rng_state: 0xDEAD_BEEF_CAFE_BABEu64,
        }
    }

    pub fn generate_samples(&mut self, count: usize, amplitude: f32, frequency: f32) {
        for _ in 0..count {
            let t = self.sample_index as f32 / self.sample_rate;

            let base = amplitude * (2.0 * std::f32::consts::PI * frequency * t).sin();

            let noise = self.random_normal() * 0.05 * amplitude;

            let transient = if self.random_u32() % 1000 == 0 {
                amplitude
                    * 2.0
                    * if self.random_u32() % 2 == 0 {
                        1.0
                    } else {
                        -1.0
                    }
            } else {
                0.0
            };

            let sample = base + noise + transient;

            self.buffer[self.write_pos as usize] = sample;
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
        // Box-Muller-ish approximation: sum of 6 uniforms - 3
        let mut sum = 0.0f32;
        for _ in 0..6 {
            sum += (self.random_u32() as f32) / (u32::MAX as f32);
        }
        sum - 3.0
    }
}
