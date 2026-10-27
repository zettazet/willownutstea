/// Bounded application gain, independent of the Windows system mixer.
pub fn apply_gain(sample: f32, percent: u32) -> f32 {
    limit(sample * percent.min(200) as f32 * 0.01)
}
// Gentle peak limiting prevents hard clipping at high gain, without per-sample allocations.
fn limit(sample: f32) -> f32 {
    let magnitude = sample.abs();
    if magnitude <= 0.9 {
        sample
    } else {
        sample.signum() * (0.9 + (magnitude - 0.9) / (1. + 10. * (magnitude - 0.9)))
    }
}
use mini_shared::{FRAME, MAX_OPUS};
use std::collections::{HashMap, VecDeque};
const JITTER_FRAMES: usize = 6;
#[derive(Clone)]
pub struct AudioPacket {
    pub bytes: [u8; MAX_OPUS],
    pub len: usize,
}
struct BufferedPacket {
    sequence: u64,
    packet: AudioPacket,
}
pub struct Speaker {
    decoder: opus::Decoder,
    packets: VecDeque<BufferedPacket>,
    next: u64,
    pcm: [f32; FRAME],
    pos: usize,
    started: bool,
    lost: u32,
    gain: f32,
}
impl Speaker {
    fn new(sequence: u64) -> anyhow::Result<Self> {
        Ok(Self {
            decoder: opus::Decoder::new(48000, opus::Channels::Mono)?,
            packets: VecDeque::with_capacity(JITTER_FRAMES),
            next: sequence,
            pcm: [0.; FRAME],
            pos: FRAME,
            started: false,
            lost: 0,
            gain: 1.,
        })
    }
    fn packet(&mut self, sequence: u64, data: &[u8]) -> anyhow::Result<()> {
        anyhow::ensure!(
            data.len() <= MAX_OPUS && opus::packet::get_nb_samples(data, 48000)? == FRAME,
            "Expected a 20ms Opus packet"
        );
        if sequence < self.next {
            return Ok(());
        }
        if sequence.saturating_sub(self.next) > 50 {
            self.packets.clear();
            self.next = sequence;
            self.started = false;
            self.pos = FRAME;
        }
        if self.packets.iter().any(|p| p.sequence == sequence) {
            return Ok(());
        }
        if self.packets.len() == JITTER_FRAMES {
            self.packets.pop_front();
            if let Some(p) = self.packets.front() {
                self.next = p.sequence;
            }
        }
        let mut bytes = [0u8; MAX_OPUS];
        bytes[..data.len()].copy_from_slice(data);
        let at = self
            .packets
            .iter()
            .position(|p| p.sequence > sequence)
            .unwrap_or(self.packets.len());
        self.packets.insert(
            at,
            BufferedPacket {
                sequence,
                packet: AudioPacket {
                    bytes,
                    len: data.len(),
                },
            },
        );
        Ok(())
    }
    fn sample(&mut self) -> f32 {
        if !self.started {
            if self.packets.len() < 2 {
                return 0.;
            }
            self.started = true;
        }
        if self.pos == FRAME {
            while self.packets.front().is_some_and(|p| p.sequence < self.next) {
                self.packets.pop_front();
            }
            let result = if self
                .packets
                .front()
                .is_some_and(|p| p.sequence == self.next)
            {
                let p = self.packets.pop_front().unwrap();
                self.lost = 0;
                self.decoder
                    .decode_float(&p.packet.bytes[..p.packet.len], &mut self.pcm, false)
            } else {
                self.lost += 1;
                if self.lost > 3 {
                    self.started = false;
                    self.pcm.fill(0.);
                    if let Some(p) = self.packets.front() {
                        self.next = p.sequence;
                    }
                    return 0.;
                }
                self.decoder.decode_float(&[], &mut self.pcm, false)
            };
            if result.is_err() {
                self.pcm.fill(0.);
            }
            self.next = self.next.saturating_add(1);
            self.pos = 0;
        }
        let sample = self.pcm[self.pos];
        self.pos += 1;
        sample
    }
}
pub struct Mixer {
    pub speakers: HashMap<i64, Speaker>,
    pub gains: HashMap<i64, f32>,
    pub master: f32,
}
impl Default for Mixer {
    fn default() -> Self {
        Self::new()
    }
}
impl Mixer {
    pub fn new() -> Self {
        Self {
            speakers: HashMap::with_capacity(7),
            gains: HashMap::with_capacity(7),
            master: 1.,
        }
    }
    pub fn packet(&mut self, user: i64, sequence: u64, data: &[u8]) -> anyhow::Result<()> {
        if !self.speakers.contains_key(&user) {
            if self.speakers.len() >= 7 {
                return Ok(());
            }
            let mut speaker = Speaker::new(sequence)?;
            speaker.gain = self.gains.get(&user).copied().unwrap_or(1.);
            self.speakers.insert(user, speaker);
        }
        self.speakers.get_mut(&user).unwrap().packet(sequence, data)
    }
    pub fn set_user_gain(&mut self, user: i64, gain: f32) {
        let gain = gain.clamp(0., 2.);
        self.gains.insert(user, gain);
        if let Some(speaker) = self.speakers.get_mut(&user) {
            speaker.gain = gain;
        }
    }
    pub fn sample(&mut self) -> f32 {
        limit(
            self.speakers
                .values_mut()
                .map(|speaker| speaker.sample() * speaker.gain)
                .sum::<f32>()
                * self.master.clamp(0., 2.),
        )
    }
}
#[cfg(windows)]
pub mod device {
    use super::*;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex,
    };
    pub struct Audio {
        _input: cpal::Stream,
        _output: cpal::Stream,
        pub mic: Arc<AtomicBool>,
        pub deaf: Arc<AtomicBool>,
        pub failed: Arc<AtomicBool>,
        pub input_gain: Arc<AtomicU32>,
        pub cleanup: Arc<AtomicBool>,
        pub neural: Arc<AtomicBool>,
        pub threshold: Arc<AtomicU32>,
    }
    fn preflight() -> anyhow::Result<()> {
        // CPAL 0.16 unwraps COM activation internally. Check it first so a missing
        // Windows audio service/driver produces an error instead of aborting.
        use windows::Win32::{
            Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator},
            System::Com::*,
        };
        unsafe {
            let initialized = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
            let result =
                CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .map(|_| ());
            if initialized {
                CoUninitialize();
            }
            result.map_err(|e| anyhow::anyhow!("Windows audio service unavailable: {e}"))
        }
    }
    pub fn devices(input: bool) -> Vec<String> {
        if preflight().is_err() {
            return vec![];
        }
        let host = cpal::default_host();
        let list = if input {
            host.input_devices()
        } else {
            host.output_devices()
        };
        list.map(|devices| devices.filter_map(|d| d.name().ok()).collect())
            .unwrap_or_default()
    }
    fn select(
        host: &cpal::Host,
        name: &Option<String>,
        input: bool,
    ) -> anyhow::Result<cpal::Device> {
        if let Some(name) = name {
            let devices = if input {
                host.input_devices()?
            } else {
                host.output_devices()?
            };
            if let Some(device) = devices
                .into_iter()
                .find(|d| d.name().ok().as_ref() == Some(name))
            {
                return Ok(device);
            }
        }
        (if input {
            host.default_input_device()
        } else {
            host.default_output_device()
        })
        .ok_or_else(|| anyhow::anyhow!("No available audio device"))
    }
    impl Audio {
        pub fn open(
            tx: tokio::sync::mpsc::Sender<AudioPacket>,
            mixer: Arc<Mutex<Mixer>>,
            preferences: &crate::preferences::Preferences,
            mic_muted: bool,
            deafened: bool,
        ) -> anyhow::Result<Self> {
            preflight()?;
            let host = cpal::default_host();
            let input = select(&host, &preferences.input_device, true)?;
            let output = select(&host, &preferences.output_device, false)?;
            let ic = input.default_input_config()?;
            let oc = output.default_output_config()?;
            let mic = Arc::new(AtomicBool::new(mic_muted));
            let deaf = Arc::new(AtomicBool::new(deafened));
            let failed = Arc::new(AtomicBool::new(false));
            let mut enc = opus::Encoder::new(48000, opus::Channels::Mono, opus::Application::Voip)?;
            enc.set_bitrate(opus::Bitrate::Bits(24000))?;
            enc.set_complexity(3)?;
            enc.set_inband_fec(true)?;
            enc.set_packet_loss_perc(5)?;
            let rate = ic.sample_rate().0;
            let channels = ic.channels() as usize;
            let m = mic.clone();
            let input_gain = Arc::new(AtomicU32::new(preferences.microphone.min(200) as u32));
            let gain = input_gain.clone();
            let cleanup = Arc::new(AtomicBool::new(preferences.voice_cleanup));
            let cleanup_enabled = cleanup.clone();
            let neural = Arc::new(AtomicBool::new(preferences.neural_cleanup));
            let neural_enabled = neural.clone();
            let threshold = Arc::new(AtomicU32::new(
                10f32
                    .powf(preferences.noise_threshold_db.clamp(-60, -20) as f32 / 20.)
                    .to_bits(),
            ));
            let threshold_value = threshold.clone();
            let mut cleaner = crate::noise_cleaner::NoiseCleaner::new();
            let mut filter = crate::voice_filter::VoiceFilter::new();
            let mut was_muted = false;
            let mut frame = [0f32; FRAME];
            let mut used = 0;
            let mut phase = 0u64;
            let mut packet = [0u8; mini_shared::MAX_OPUS];
            // Fixed buffers and bounded handoff. Resampling uses a bounded linear converter.
            let mut previous = 0f32;
            let mut capture = move |value: f32| {
                if m.load(Ordering::Relaxed) {
                    if !was_muted {
                        filter.reset();
                        cleaner.reset();
                        frame.fill(0.);
                        previous = 0.;
                        was_muted = true;
                    }
                    used = 0;
                    phase = 0;
                    return;
                }
                was_muted = false;
                phase += 48000;
                while phase >= rate as u64 {
                    phase -= rate as u64;
                    let blend = phase as f32 / 48000.;
                    filter.set_threshold(f32::from_bits(threshold_value.load(Ordering::Relaxed)));
                    frame[used] = filter.sample(
                        value * (1. - blend) + previous * blend,
                        cleanup_enabled.load(Ordering::Relaxed),
                    );
                    used += 1;
                    if used == FRAME {
                        cleaner.process(
                            &mut frame,
                            cleanup_enabled.load(Ordering::Relaxed)
                                && neural_enabled.load(Ordering::Relaxed),
                        );
                        let input_percent = gain.load(Ordering::Relaxed);
                        for sample in &mut frame {
                            *sample = apply_gain(*sample, input_percent);
                        }
                        if let Ok(n) = enc.encode_float(&frame, &mut packet) {
                            let _ = tx.try_send(AudioPacket {
                                bytes: packet,
                                len: n,
                            });
                        }
                        used = 0;
                    }
                }
                previous = value;
            };
            let input_config: cpal::StreamConfig = ic.clone().into();
            macro_rules! input_stream {
                ($ty:ty) => {{
                    let f = failed.clone();
                    input.build_input_stream(
                        &input_config,
                        move |data: &[$ty], _| {
                            for frame in data.chunks(channels) {
                                let mono = frame
                                    .iter()
                                    .map(|x| cpal::Sample::to_sample::<f32>(*x))
                                    .sum::<f32>()
                                    / channels as f32;
                                capture(mono);
                            }
                        },
                        move |_| {
                            f.store(true, Ordering::Relaxed);
                        },
                        None,
                    )?
                }};
            }
            let input_stream = match ic.sample_format() {
                cpal::SampleFormat::F32 => input_stream!(f32),
                cpal::SampleFormat::I16 => input_stream!(i16),
                cpal::SampleFormat::U16 => input_stream!(u16),
                cpal::SampleFormat::I32 => input_stream!(i32),
                cpal::SampleFormat::F64 => input_stream!(f64),
                _ => return Err(anyhow::anyhow!("Unsupported microphone sample format")),
            };
            let rate = oc.sample_rate().0;
            let channels = oc.channels() as usize;
            let output_config: cpal::StreamConfig = oc.clone().into();
            let mut phase = 0u64;
            let mut current = 0f32;
            macro_rules! output_stream {
                ($ty:ty) => {{
                    let mix = mixer.clone();
                    let d = deaf.clone();
                    let f = failed.clone();
                    output.build_output_stream(
                        &output_config,
                        move |data: &mut [$ty], _| {
                            let Ok(mut mix) = mix.try_lock() else {
                                data.fill(<$ty as cpal::Sample>::EQUILIBRIUM);
                                return;
                            };
                            for frame in data.chunks_mut(channels) {
                                phase += 48000;
                                while phase >= rate as u64 {
                                    phase -= rate as u64;
                                    current = mix.sample();
                                }
                                let value = if d.load(Ordering::Relaxed) {
                                    0.
                                } else {
                                    current
                                };
                                frame.fill(<$ty as cpal::FromSample<f32>>::from_sample_(value));
                            }
                        },
                        move |_| {
                            f.store(true, Ordering::Relaxed);
                        },
                        None,
                    )?
                }};
            }
            let output_stream = match oc.sample_format() {
                cpal::SampleFormat::F32 => output_stream!(f32),
                cpal::SampleFormat::I16 => output_stream!(i16),
                cpal::SampleFormat::U16 => output_stream!(u16),
                cpal::SampleFormat::I32 => output_stream!(i32),
                cpal::SampleFormat::F64 => output_stream!(f64),
                _ => return Err(anyhow::anyhow!("Unsupported speaker sample format")),
            };
            input_stream.play()?;
            output_stream.play()?;
            Ok(Self {
                _input: input_stream,
                _output: output_stream,
                mic,
                deaf,
                failed,
                input_gain,
                cleanup,
                neural,
                threshold,
            })
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opus_transport_mix_and_bounds() {
        let mut enc =
            opus::Encoder::new(48000, opus::Channels::Mono, opus::Application::Voip).unwrap();
        let mut pcm = [0f32; FRAME];
        for (i, x) in pcm.iter_mut().enumerate() {
            *x = (i as f32 * 440. * std::f32::consts::TAU / 48000.).sin() * 0.2;
        }
        let mut packet = [0u8; 1275];
        let n = enc.encode_float(&pcm, &mut packet).unwrap();
        let mut m = Mixer::new();
        for seq in 0..100 {
            m.packet(1, seq, &packet[..n]).unwrap();
        }
        assert!(m.speakers[&1].packets.len() <= JITTER_FRAMES);
        assert!((0..FRAME).any(|_| m.sample().abs() > 0.001));
        assert!(m.packet(1, 101, &[255; 1275]).is_err());
    }
    #[test]
    fn multiple_speakers_reordering_loss_and_cleanup() {
        let mut encoder =
            opus::Encoder::new(48000, opus::Channels::Mono, opus::Application::Voip).unwrap();
        let samples: Vec<f32> = (0..FRAME).map(|i| (i as f32 * 0.09).sin() * 0.4).collect();
        let mut packet = [0u8; MAX_OPUS];
        let n = encoder.encode_float(&samples, &mut packet).unwrap();
        let mut mixer = Mixer::new();
        for user in 1..=7 {
            for sequence in [0, 2, 1, 2, 4] {
                mixer.packet(user, sequence, &packet[..n]).unwrap();
            }
        }
        assert_eq!(mixer.speakers.len(), 7);
        assert!(mixer.speakers.values().all(|s| s.packets.len() == 4));
        let mut nonzero = 0;
        for _ in 0..FRAME * 12 {
            let s = mixer.sample();
            assert!(s.is_finite() && (-1.0..=1.0).contains(&s));
            if s.abs() > 0.01 {
                nonzero += 1;
            }
        }
        assert!(nonzero > FRAME);
        assert!(mixer.speakers.values().all(|s| !s.started));
        mixer.speakers.clear();
        assert_eq!(mixer.sample(), 0.);
    }
}

#[cfg(test)]
mod gain_tests {
    use super::*;
    #[test]
    fn capture_master_and_independent_participant_gains() {
        assert!(apply_gain(0.75, 200) > 0.9 && apply_gain(0.75, 200) < 1.);
        assert!((apply_gain(0.5, 40) - 0.2).abs() < 0.000001);
        assert_eq!(apply_gain(0.5, 0), 0.);
        let mut encoder =
            opus::Encoder::new(48000, opus::Channels::Mono, opus::Application::Voip).unwrap();
        let pcm: Vec<f32> = (0..FRAME).map(|i| (i as f32 * 0.09).sin() * 0.1).collect();
        let mut bytes = [0; MAX_OPUS];
        let n = encoder.encode_float(&pcm, &mut bytes).unwrap();
        let mut one = Mixer::new();
        let mut two = Mixer::new();
        for seq in 0..4 {
            for mix in [&mut one, &mut two] {
                for id in [1, 2] {
                    mix.packet(id, seq, &bytes[..n]).unwrap();
                }
            }
        }
        two.set_user_gain(1, 0.);
        two.set_user_gain(2, 0.4);
        two.master = 1.5;
        for _ in 0..FRAME {
            assert!((two.sample() - one.sample() * 0.3).abs() < 0.00001);
        }
        // Change one gain while decoders are already playing buffered packets.
        two.set_user_gain(2, 0.8);
        for _ in 0..FRAME {
            assert!((two.sample() - one.sample() * 0.6).abs() < 0.00001);
        }
        two.master = 0.;
        assert_eq!(two.sample(), 0.);
    }
}
