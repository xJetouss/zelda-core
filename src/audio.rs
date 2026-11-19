//! Audioausgabe über cpal mit adaptivem Resampling (32 kHz -> Geräterate).

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

const SRC_RATE: f64 = 32_000.0;

struct Shared {
    buf: VecDeque<(f32, f32)>,
    target: usize,
    volume: f32,
    frac: f64,
    prev: (f32, f32),
    next: (f32, f32),
}

pub struct AudioOut {
    _stream: cpal::Stream,
    shared: Arc<Mutex<Shared>>,
    pub device_name: String,
    pub device_rate: u32,
}

impl AudioOut {
    pub fn new(latency_ms: u32) -> Option<Self> {
        let host = cpal::default_host();
        let dev = host.default_output_device()?;
        let cfg = dev.default_output_config().ok()?;
        let rate = cfg.sample_rate().0;
        let ch = cfg.channels() as usize;
        let shared = Arc::new(Mutex::new(Shared {
            buf: VecDeque::with_capacity(16384),
            target: latency_to_samples(latency_ms),
            volume: 1.0,
            frac: 0.0,
            prev: (0.0, 0.0),
            next: (0.0, 0.0),
        }));
        let fmt = cfg.sample_format();
        let sc: cpal::StreamConfig = cfg.into();
        let s2 = shared.clone();
        let stream = match fmt {
            cpal::SampleFormat::F32 => build::<f32>(&dev, &sc, ch, rate, s2),
            cpal::SampleFormat::I16 => build::<i16>(&dev, &sc, ch, rate, s2),
            cpal::SampleFormat::U16 => build::<u16>(&dev, &sc, ch, rate, s2),
            _ => return None,
        }
        .ok()?;
        stream.play().ok()?;
        Some(AudioOut {
            _stream: stream,
            shared,
            device_name: dev.name().unwrap_or_default(),
            device_rate: rate,
        })
    }

    pub fn push(&self, samples: &[i16]) {
        let mut s = self.shared.lock().unwrap();
        for p in samples.chunks_exact(2) {
            s.buf.push_back((p[0] as f32 / 32768.0, p[1] as f32 / 32768.0));
        }
        let max = s.target * 3 + 1024;
        if s.buf.len() > max {
            let drop = s.buf.len() - s.target;
            s.buf.drain(..drop);
        }
    }

    pub fn set_params(&self, volume: f32, latency_ms: u32) {
        let mut s = self.shared.lock().unwrap();
        s.volume = volume;
        s.target = latency_to_samples(latency_ms);
    }

    pub fn clear(&self) {
        self.shared.lock().unwrap().buf.clear();
    }
}

fn latency_to_samples(ms: u32) -> usize {
    ((ms.max(10) as f64) * SRC_RATE / 1000.0) as usize
}

fn build<T: cpal::SizedSample + cpal::FromSample<f32>>(
    dev: &cpal::Device,
    cfg: &cpal::StreamConfig,
    ch: usize,
    rate: u32,
    sh: Arc<Mutex<Shared>>,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    let base = SRC_RATE / rate as f64;
    dev.build_output_stream(
        cfg,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let mut guard = sh.lock().unwrap();
            let s = &mut *guard;
            let fill = s.buf.len() as f64;
            let target = s.target.max(1) as f64;
            let adj = 1.0 + ((fill - target) / target).clamp(-1.0, 1.0) * 0.005;
            let step = base * adj;
            for frame in data.chunks_mut(ch) {
                while s.frac >= 1.0 {
                    s.frac -= 1.0;
                    s.prev = s.next;
                    if let Some(n) = s.buf.pop_front() {
                        s.next = n;
                    }
                }
                let t = s.frac as f32;
                let l = (s.prev.0 + (s.next.0 - s.prev.0) * t) * s.volume;
                let r = (s.prev.1 + (s.next.1 - s.prev.1) * t) * s.volume;
                s.frac += step;
                for (i, out) in frame.iter_mut().enumerate() {
                    let v = match i {
                        0 => l,
                        1 => r,
                        _ => 0.0,
                    };
                    *out = T::from_sample(v.clamp(-1.0, 1.0));
                }
            }
        },
        |e| eprintln!("Audiofehler: {e}"),
        None,
    )
}
