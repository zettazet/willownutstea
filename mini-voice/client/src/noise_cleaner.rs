//! One RNNoise state per microphone, 10ms frames, built-in read-only model.
//! No queues, worker threads or model downloads. State resets allocate only on
//! mute/bypass transitions; steady processing reuses state and FFT scratch.
use nnnoiseless::DenoiseState;
pub struct NoiseCleaner {
    state: Box<DenoiseState<'static>>,
    input: [f32; 480],
    output: [f32; 480],
    dirty: bool,
    enabled: bool,
}
impl Default for NoiseCleaner {
    fn default() -> Self {
        Self::new()
    }
}
impl NoiseCleaner {
    pub fn new() -> Self {
        Self {
            state: DenoiseState::new(),
            input: [0.; 480],
            output: [0.; 480],
            dirty: true,
            enabled: true,
        }
    }
    pub fn reset(&mut self) {
        // Recreate recurrent, pitch and overlap state, so none of the pre-mute
        // waveform can return after unmuting. Only done once per transition.
        self.state = DenoiseState::new();
        self.dirty = true;
        self.input.fill(0.);
        self.output.fill(0.);
    }
    pub fn process(&mut self, samples: &mut [f32; mini_shared::FRAME], enabled: bool) {
        if enabled != self.enabled {
            self.reset();
            self.enabled = enabled;
        }
        if !enabled {
            return;
        }
        for chunk in samples.as_chunks_mut::<480>().0 {
            for (to, from) in self.input.iter_mut().zip(chunk.iter()) {
                *to = if from.is_finite() {
                    from.clamp(-1., 0.9999695) * 32768.
                } else {
                    0.
                };
            }
            self.state.process_frame(&mut self.output, &self.input);
            for (to, from) in chunk.iter_mut().zip(self.output.iter()) {
                *to = if self.dirty || !from.is_finite() {
                    0.
                } else {
                    (from / 32768.).clamp(-1., 1.)
                };
            }
            self.dirty = false;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn silence_bypass_reset_and_finite_output() {
        let mut c = NoiseCleaner::new();
        let mut samples = [0.; mini_shared::FRAME];
        c.process(&mut samples, true);
        assert!(samples.iter().all(|s| s.abs() < 1e-6));
        samples.fill(0.125);
        c.process(&mut samples, false);
        assert_eq!(samples, [0.125; mini_shared::FRAME]);
        for _ in 0..8 {
            for (i, x) in samples.iter_mut().enumerate() {
                *x = (i as f32 * 0.15).sin() * 0.2;
            }
            c.process(&mut samples, true);
            assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 1.));
        }
        c.reset();
        samples.fill(0.);
        c.process(&mut samples, true);
        assert!(samples.iter().all(|s| s.abs() < 1e-6));
    }
}
