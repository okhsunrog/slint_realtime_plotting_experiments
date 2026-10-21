/// A sample clock with a small receive buffer. Rendering follows time rather
/// than packet arrivals; data is never extrapolated beyond received samples.
#[derive(Default)]
pub struct PlaybackClock {
    received: u64,
    head: u64,
    anchor: Option<(f64, u64)>,
    pub underruns: u64,
}
impl PlaybackClock {
    pub fn received(&self) -> u64 {
        self.received
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn receive(&mut self, frames: usize, now_ms: f64) {
        self.received += frames as u64;
        if self.anchor.is_none() {
            self.anchor = Some((now_ms, self.received));
            self.head = self.received;
        }
    }
    pub fn offset(
        &mut self,
        now_ms: f64,
        rate: u32,
        batch: usize,
        filled: usize,
        visible: usize,
    ) -> usize {
        let Some((at, head)) = self.anchor else {
            return 0;
        };
        let lead = (batch as u64 * 2) + (rate as u64).div_ceil(60);
        let elapsed = ((now_ms - at).max(0.) * rate as f64 / 1000.) as u64;
        let desired = head.saturating_add(elapsed).saturating_sub(lead);
        if desired > self.received {
            self.underruns += 1;
            self.anchor = Some((now_ms, self.received));
        }
        // Keep the full requested window inside the retained ring history.
        // At full-ring zoom no history remains available for buffering.
        let available = filled.saturating_sub(visible.min(filled)) as u64;
        self.head = self
            .head
            .max(desired.min(self.received))
            .max(self.received.saturating_sub(available));
        (self.received - self.head) as usize
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn head_moves_with_time_between_packets() {
        let mut c = PlaybackClock::default();
        c.receive(100, 0.);
        c.receive(100, 100.);
        c.receive(100, 200.);
        c.offset(220., 1000, 100, 300, 50);
        let a = c.received - c.offset(230., 1000, 100, 300, 50) as u64;
        let b = c.received - c.offset(240., 1000, 100, 300, 50) as u64;
        assert_eq!(b - a, 10);
    }
    #[test]
    fn packet_arrival_does_not_jump_the_render_head() {
        let mut c = PlaybackClock::default();
        c.receive(100, 0.);
        c.receive(100, 100.);
        c.receive(100, 200.);
        let a = c.received - c.offset(240., 1000, 100, 300, 50) as u64;
        c.receive(100, 240.);
        let b = c.received - c.offset(240., 1000, 100, 400, 50) as u64;
        assert_eq!(a, b);
    }
    #[test]
    fn underrun_does_not_render_missing_samples_and_rebuffers() {
        let mut c = PlaybackClock::default();
        c.receive(100, 0.);
        assert_eq!(c.offset(1000., 1000, 100, 100, 50), 0);
        assert_eq!(c.underruns, 1);
        c.receive(100, 1010.);
        assert_eq!(c.offset(1010., 1000, 100, 200, 50), 100);
    }
    #[test]
    fn history_wrap_and_full_ring_zoom_remain_in_bounds() {
        let mut c = PlaybackClock::default();
        c.receive(1000, 0.);
        c.receive(500, 10.);
        assert!(c.offset(10., 1000, 100, 1000, 800) <= 200);
        assert_eq!(c.offset(10., 1000, 100, 1000, 1000), 0);
        c.reset();
        assert_eq!(c.offset(10., 1000, 100, 0, 1000), 0);
    }
}
