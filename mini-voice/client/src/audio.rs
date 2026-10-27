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
        }
    }
    pub fn packet(&mut self, user: i64, sequence: u64, data: &[u8]) -> anyhow::Result<()> {
        if !self.speakers.contains_key(&user) {
            if self.speakers.len() >= 7 {
                return Ok(());
            }
            self.speakers.insert(user, Speaker::new(sequence)?);
        }
        self.speakers.get_mut(&user).unwrap().packet(sequence, data)
    }
    pub fn sample(&mut self) -> f32 {
        self.speakers
            .values_mut()
            .map(Speaker::sample)
            .sum::<f32>()
            .clamp(-1., 1.)
    }
}
#[cfg(windows)]
pub mod device {
    use super::*;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    };
    pub struct Audio {
        _input: cpal::Stream,
        _output: cpal::Stream,
        pub mic: Arc<AtomicBool>,
        pub deaf: Arc<AtomicBool>,
        pub failed: Arc<AtomicBool>,
    }
    impl Audio {
        pub fn open(
            tx: tokio::sync::mpsc::Sender<AudioPacket>,
            mixer: Arc<Mutex<Mixer>>,
        ) -> anyhow::Result<Self> {
            let host = cpal::default_host();
            let input = host
                .default_input_device()
                .ok_or_else(|| anyhow::anyhow!("No microphone"))?;
            let output = host
                .default_output_device()
                .ok_or_else(|| anyhow::anyhow!("No speakers"))?;
            let ic = input.default_input_config()?;
            let oc = output.default_output_config()?;
            let mic = Arc::new(AtomicBool::new(false));
            let deaf = Arc::new(AtomicBool::new(false));
            let failed = Arc::new(AtomicBool::new(false));
            let mut enc = opus::Encoder::new(48000, opus::Channels::Mono, opus::Application::Voip)?;
            enc.set_bitrate(opus::Bitrate::Bits(24000))?;
            enc.set_complexity(3)?;
            enc.set_inband_fec(true)?;
            enc.set_packet_loss_perc(5)?;
            let rate = ic.sample_rate().0;
            let channels = ic.channels() as usize;
            let m = mic.clone();
            let mut frame = [0f32; FRAME];
            let mut used = 0;
            let mut phase = 0u64;
            let mut packet = [0u8; mini_shared::MAX_OPUS];
            // Fixed buffers and bounded handoff. Resampling uses a bounded linear converter.
            let mut previous = 0f32;
            let mut capture = move |value: f32| {
                if m.load(Ordering::Relaxed) {
                    used = 0;
                    phase = 0;
                    return;
                }
                phase += 48000;
                while phase >= rate as u64 {
                    phase -= rate as u64;
                    let blend = phase as f32 / 48000.;
                    frame[used] = value * (1. - blend) + previous * blend;
                    used += 1;
                    if used == FRAME {
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
