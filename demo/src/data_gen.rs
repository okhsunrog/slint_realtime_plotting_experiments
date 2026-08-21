use slint_realtime_plot::PlotBuffer;

pub const NUM_SAMPLES: usize = 32768;
pub const NUM_CHANNELS: usize = 3;
pub const SAMPLE_RATE: f32 = 20000.0;

const TWO_PI: f32 = 2.0 * std::f32::consts::PI;

const PHASE_OFFSETS: [f32; NUM_CHANNELS] = [0.0, TWO_PI / 3.0, 2.0 * TWO_PI / 3.0];

pub struct MotorSimulator {
    /// Reused staging area for one batch of interleaved frames.
    scratch: Vec<f32>,
    /// Accumulated electrical angle — frequency changes stay phase-continuous.
    phase: f32,
    sample_rate: f32,
    rng_state: u64,
}

impl MotorSimulator {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            scratch: Vec::new(),
            phase: 0.0,
            sample_rate,
            rng_state: 0xDEAD_BEEF_CAFE_BABEu64,
        }
    }

    pub fn generate_samples(
        &mut self,
        buffer: &PlotBuffer,
        count: usize,
        amplitude: f32,
        frequency: f32,
    ) {
        let phase_step = TWO_PI * frequency / self.sample_rate;
        self.scratch.clear();
        for _ in 0..count {
            self.phase = (self.phase + phase_step) % TWO_PI;

            for &offset in &PHASE_OFFSETS {
                let phase_current = amplitude * (self.phase + offset).sin();
                let noise = self.random_normal() * 0.05 * amplitude;
                self.scratch.push(phase_current + noise);
            }
        }
        buffer.push_batch(&self.scratch);
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
