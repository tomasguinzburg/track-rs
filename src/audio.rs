#![allow(clippy::precedence)]
use crate::tracker::Note;
use anyhow::{anyhow, Context};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, FromSample, SizedSample, Stream, StreamConfig, SupportedStreamConfig,
};
use fundsp::{
    hacker::{
        chorus, dcblock, follow, highshelf_hz, lfo, moog, multipass, pass, pluck, reverb2_stereo, shared, split, zero,
        AudioUnit, BlockRateAdapter, Fade, Sequencer, U2,
    },
    math::{cos_hz, db_amp, midi_hz, sin_hz, xerp11, AttoHash},
    net::Net,
    prelude::{pan, var},
    realseq::SequencerBackend,
};
use std::{f32, time::Duration};

pub struct AudioEngine {
    sequencer: Sequencer,
    stream: Stream,
}

impl AudioEngine {
    //TODO: get the config and device inside
    pub fn new() -> anyhow::Result<Self> {
        let host = cpal::default_host();

        let device = host
            .default_output_device()
            .expect("failed to find a default output device");
        let config = device.default_output_config().expect("no default output config");

        let sample_rate = config.sample_rate().0 as f64;

        let mut sequencer = Sequencer::new(true, 1);
        let sequencer_backend = sequencer.backend();

        //TODO: move to voice, this builds a fundsp net:
        let mut fx_net = build_net(sample_rate, sequencer_backend);

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => build_stream::<f32>(config, &device, &mut fx_net)?,
            cpal::SampleFormat::I16 => build_stream::<i16>(config, &device, &mut fx_net)?,
            cpal::SampleFormat::U16 => build_stream::<u16>(config, &device, &mut fx_net)?,
            _ => panic!("Unsupported sample format"),
        };

        stream.play()?;
        Ok(AudioEngine { sequencer, stream })
    }

    pub fn play(&mut self, note: Note) {
        let pitch_hz = note.freq();
        // let pitch = lfo(move |t| pitch_hz * xerp11(1.0, 1.0, 0.5 * (sin_hz(6.0, t) + sin_hz(6.1, t))));
        let waveform = Net::wrap(Box::new(zero() >> pluck(pitch_hz as f32, 0.5, 0.5) * 0.5));
        let filter = Net::wrap(Box::new(
            (pass() | lfo(move |t| (xerp11(400.0, 10000.0, cos_hz(0.1, t)), 0.6))) >> moog(),
        ));

        let mut note = Box::new(waveform >> filter >> dcblock());
        let seed: u64 = rand::random();
        // Give the note its own random seed.
        note.ping(false, AttoHash::new(seed));
        // TODO: end_time should be based on note lenght
        self.sequencer.push_relative(0.0, 0.9, Fade::Smooth, 0.02, 0.2, note);
    }
}

fn build_stream<T>(config: SupportedStreamConfig, device: &Device, fx_net: &mut Net) -> anyhow::Result<Stream>
where
    T: SizedSample + cpal::FromSample<f64>,
{
    let mut fx_net_backend = BlockRateAdapter::new(Box::new(fx_net.backend()));
    let mut next_value = move || fx_net_backend.get_stereo();
    let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

    let channels = config.channels() as usize;
    let stream = device.build_output_stream(
        &config.into(),
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| write_data(data, channels, &mut next_value),
        err_fn,
        None,
    )?;

    Ok(stream)
}

fn write_data<T>(output: &mut [T], channels: usize, next_sample: &mut dyn FnMut() -> (f32, f32))
where
    T: SizedSample + FromSample<f64>,
{
    for frame in output.chunks_mut(channels) {
        let sample = next_sample();
        let left: T = T::from_sample(sample.0 as f64);
        let right: T = T::from_sample(sample.1 as f64);

        for (channel, sample) in frame.iter_mut().enumerate() {
            if channel & 1 == 0 {
                *sample = left;
            } else {
                *sample = right;
            }
        }
    }
}

fn build_net(sample_rate: f64, sequencer_backend: SequencerBackend) -> Net {
    let room_size = 10.0;
    let reverb_amount = shared(0.25);
    let reverb_time = 2.0;
    let reverb_diffusion = 0.5;
    let chorus_amount = shared(1.0);

    let mut net = Net::wrap(Box::new(sequencer_backend));
    let (reverb, reverb_id) = Net::wrap_id(create_reverb(room_size, reverb_time, reverb_diffusion));
    let (phaser, phaser_id) = Net::wrap_id(Box::new(multipass::<U2>()));
    let (flanger, flanger_id) = Net::wrap_id(Box::new(multipass::<U2>()));
    net = net >> pan(0.0);
    // Smooth chorus and reverb amounts to prevent discontinuities.
    net = net
        >> ((1.0 - var(&chorus_amount) >> follow(0.01) >> split()) * multipass()
            & (var(&chorus_amount) >> follow(0.01) >> split())
                * 2.0
                * (chorus(0, 0.0, 0.03, 0.2) | chorus(1, 0.0, 0.03, 0.2)));
    net = net >> phaser >> flanger;
    net = net
        >> ((1.0 - var(&reverb_amount) >> follow(0.01) >> split::<U2>()) * multipass()
            & (var(&reverb_amount) >> follow(0.01) >> split::<U2>()) * reverb);

    net.set_sample_rate(sample_rate);

    net
}

fn create_reverb(room_size: f32, time: f32, diffusion: f32) -> Box<dyn AudioUnit> {
    //Box::new(reverb3_stereo(time, diffusion, highshelf_hz(5000.0, 1.0, db_amp(-1.0))))
    Box::new(reverb2_stereo(
        room_size,
        time,
        diffusion,
        1.0,
        highshelf_hz(5000.0, 1.0, db_amp(-1.0)),
    ))
    //Box::new(reverb4_stereo(room_size, time))
}
