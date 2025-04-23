use crate::tracker::Note;
use anyhow::{anyhow, Context};
use rodio::{
    cpal::{self, traits::HostTrait},
    DeviceTrait, OutputStream, Sink, Source,
};
use std::time::Duration;

// START_OSCILLATORS //

struct SineWave {
    freq: f32,
    phase: f32,
    sample_rate: u32,
}

impl SineWave {
    fn new(freq: f32, sample_rate: u32) -> Self {
        SineWave {
            freq,
            phase: 0.0,
            sample_rate,
        }
    }
}

impl Source for SineWave {
    fn current_frame_len(&self) -> Option<usize> {
        None //Continuous
    }
    fn channels(&self) -> u16 {
        1 //Mono
    }
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None //Infinite
    }
}

impl Iterator for SineWave {
    type Item = f32;

    #[allow(clippy::cast_precision_loss)]
    fn next(&mut self) -> Option<f32> {
        let increment = self.freq * 2.0 * std::f32::consts::PI / self.sample_rate as f32;
        let val = (self.phase * 2.0 * std::f32::consts::PI).sin();

        // FIXME: Is this really the way?
        self.phase = (self.phase + increment) % 1.0; //Move phase to next sample
        Some(val * 0.5) //Intensity
    }
}

// END_OSCILLATORS //

pub struct AudioEngine {
    _stream: OutputStream,
    sink: Sink,
    sample_rate: u32,
}

impl AudioEngine {
    pub fn new() -> Result<Self, anyhow::Error> {
        let (stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| anyhow!("Failed to get output_stream {}", e))?;
        let sink = Sink::try_new(&stream_handle)
            .map_err(|e| anyhow!("Failed to create the sink {}", e))?;

        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .context("Failed to get output device")?;

        let sample_rate = device
            .default_output_config()
            .map(|cfg| cfg.sample_rate().0)
            .map_err(|e| anyhow!("Failed to get sample rate {}", e))?;

        Ok(AudioEngine {
            _stream: stream,
            sink,
            sample_rate,
        })
    }

    //TODO: this is sad, needs a lot of work
    pub fn play_note(&self, note: Note) {
        let source = SineWave::new(note.freq(), self.sample_rate)
            .take_duration(Duration::from_millis(150)) //TODO: Oscillators
            .amplify(0.20);

        self.sink.append(source);

        //TODO: Concurrent sounds need to be stopped on playback stop
        // For multiple simultaneous sounds without affecting speed globally, managing multiple sinks
        // or using a more complex audio graph (e.g. via `fundsp` crate) would be needed later.
    }
}
