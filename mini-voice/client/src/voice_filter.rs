//! Conservative microphone cleanup at the existing 48 kHz encoder rate.
//! Fixed state, no look-ahead, frame queues, allocations or background threads.
const RATE: f32 = 48_000.;
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    z: [f32; 2],
}
impl Biquad {
    fn new(cutoff: f32, high_pass: bool) -> Self {
        let angle = std::f32::consts::TAU * cutoff / RATE;
        let cosine = angle.cos();
        let alpha = angle.sin() / std::f32::consts::SQRT_2;
        let norm = 1. + alpha;
        let b = if high_pass {
            [(1. + cosine) / 2., -(1. + cosine), (1. + cosine) / 2.]
        } else {
            [(1. - cosine) / 2., 1. - cosine, (1. - cosine) / 2.]
        };
        Self {
            b: b.map(|v| v / norm),
            a: [-2. * cosine / norm, (1. - alpha) / norm],
            z: [0.; 2],
        }
    }
    fn sample(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        // Avoid denormal arithmetic during long silence.
        for state in &mut self.z {
            if state.abs() < 1e-20 {
                *state = 0.;
            }
        }
        y
    }
}
pub struct VoiceFilter {
    high: Biquad,
    low: Biquad,
    envelope: f32,
    gain: f32,
    enabled: bool,
    envelope_attack: f32,
    envelope_release: f32,
    gain_attack: f32,
    gain_release: f32,
}
impl Default for VoiceFilter {
    fn default() -> Self {
        Self::new()
    }
}
impl VoiceFilter {
    pub fn new() -> Self {
        let coefficient = |seconds: f32| 1. - (-1. / (RATE * seconds)).exp();
        Self {
            high: Biquad::new(80., true),
            low: Biquad::new(8000., false),
            envelope: 0.,
            gain: 0.1,
            enabled: true,
            envelope_attack: coefficient(0.002),
            envelope_release: coefficient(0.08),
            gain_attack: coefficient(0.002),
            gain_release: coefficient(0.12),
        }
    }
    pub fn reset(&mut self) {
        self.high.z = [0.; 2];
        self.low.z = [0.; 2];
        self.envelope = 0.;
        self.gain = 0.1;
    }
    pub fn sample(&mut self, input: f32, enabled: bool) -> f32 {
        if enabled != self.enabled {
            self.reset();
            self.enabled = enabled;
        }
        if !input.is_finite() {
            self.reset();
            return 0.;
        }
        if !enabled {
            return input;
        }
        let filtered = self.low.sample(self.high.sample(input));
        let level = filtered.abs();
        let speed = if level > self.envelope {
            self.envelope_attack
        } else {
            self.envelope_release
        };
        self.envelope += speed * (level - self.envelope);
        if self.envelope < 1e-20 {
            self.envelope = 0.;
        }
        // Downward expansion below about -56 dBFS; no hard on/off speech gate.
        // Never raise the noise floor or silence quiet syllables completely.
        let ratio = (self.envelope / 0.0015).min(1.);
        let target = 0.1 + 0.9 * ratio * ratio;
        let speed = if target > self.gain {
            self.gain_attack
        } else {
            self.gain_release
        };
        self.gain += speed * (target - self.gain);
        filtered * self.gain
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn response(frequency: f32, amplitude: f32) -> f32 {
        let mut filter = VoiceFilter::new();
        let mut power = 0.;
        let mut count = 0;
        for i in 0..48_000 {
            let input = amplitude * (std::f32::consts::TAU * frequency * i as f32 / RATE).sin();
            let out = filter.sample(input, true);
            assert!(out.is_finite());
            if i > 24_000 {
                power += out * out;
                count += 1;
            }
        }
        (power / count as f32).sqrt()
    }
    #[test]
    fn cleans_rumble_hiss_and_quiet_noise_without_boosting_speech() {
        let voice = response(1000., 0.1);
        assert!(voice > 0.065 && voice < 0.075);
        assert!(response(20., 0.1) < voice * 0.1);
        assert!(response(14_000., 0.1) < voice * 0.3);
        assert!(response(1000., 0.0001) < 0.00002);
        // A soft voice above the expander threshold keeps its level.
        assert!(response(1000., 0.01) > 0.0065);
    }
    #[test]
    fn bypass_mute_reset_and_long_silence_are_safe() {
        let mut filter = VoiceFilter::new();
        for _ in 0..48_000 {
            filter.sample(0.2, true);
        }
        filter.reset();
        assert_eq!(filter.sample(0., true), 0.);
        for _ in 0..48_000 {
            assert_eq!(filter.sample(0., true), 0.);
        }
        for sample in [-1., -0.1, 0., 0.1, 1.] {
            assert_eq!(filter.sample(sample, false), sample);
        }
        assert_eq!(filter.sample(f32::NAN, true), 0.);
        assert_eq!(filter.sample(f32::INFINITY, true), 0.);
        assert!(std::mem::size_of::<VoiceFilter>() < 128);
    }
}
