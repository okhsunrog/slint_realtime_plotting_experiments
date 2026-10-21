/// A sample clock with a small receive buffer. Rendering follows time rather
/// than packet arrivals; data is never extrapolated beyond received samples.
#[derive(Default)]
pub struct PlaybackClock {
    received: u64,
    head: u64,
    anchor: Option<(f64, u64)>,
    pub underruns: u64,
    pub catchups: u64,
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
        // A late callback or a burst after starvation must not leave the
        // display permanently behind the producer. Allow one batch of jitter
        // beyond the target buffer, then resume the clock from recent history.
        if self.received - self.head > lead.saturating_add(batch as u64) {
            self.head = self.received.saturating_sub(lead.min(available));
            self.anchor = Some((now_ms, self.received));
            self.catchups += 1;
        }
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
    #[test]
    fn backlog_is_bounded_and_normal_speed_resumes_after_catchup() {
        let mut c = PlaybackClock::default();
        c.receive(1000, 0.);
        c.receive(1000, 20.);
        assert_eq!(c.offset(20., 1000, 100, 2000, 50), 217);
        assert_eq!(c.catchups, 1);
        let head = c.received - 217;
        let next = c.received - c.offset(30., 1000, 100, 2000, 50) as u64;
        assert_eq!(next - head, 10);
        assert_eq!(c.catchups, 1);
    }
}
