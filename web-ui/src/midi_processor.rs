use tracing::error;
use wasm_bindgen::prelude::*;

use lamarrs_utils::midi_event::MidiEvent;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample, Stream};
use std::sync::mpsc::{Receiver, Sender};

#[wasm_bindgen]
pub struct Synth(oxisynth::Synth);

#[wasm_bindgen]
pub struct Handle(Stream, Sender<MidiEvent>);

impl Handle {
    pub fn send(&self, event: MidiEvent) {
        self.1.send(event);
    }
}

#[wasm_bindgen]
pub fn create_handler() -> Handle {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("failed to find a default output device");
    let config = device.default_output_config().unwrap();

    let (tx, rx) = std::sync::mpsc::channel::<MidiEvent>();
    Handle(
        match config.sample_format() {
            cpal::SampleFormat::F32 => run::<f32>(&device, &config.into(), rx),
            cpal::SampleFormat::I16 => run::<i16>(&device, &config.into(), rx),
            cpal::SampleFormat::U16 => run::<u16>(&device, &config.into(), rx),
            _ => panic!("Wrong Sample Format"),
        },
        tx,
    )
}

fn run<T>(device: &cpal::Device, config: &cpal::StreamConfig, rx: Receiver<MidiEvent>) -> Stream
where
    T: SizedSample + FromSample<f32>,
{
    let sample_rate = config.sample_rate.0 as f32;
    let channels = config.channels as usize;

    let mut synth = {
        let settings = oxisynth::SynthDescriptor {
            sample_rate,
            gain: 1.0,
            ..Default::default()
        };

        let mut synth = oxisynth::Synth::new(settings).unwrap();

        // Load from memory
        use std::io::Cursor;
        let mut file = Cursor::new(include_bytes!("../assets/sounds/soundfonts.sf2"));
        let font = oxisynth::SoundFont::load(&mut file).unwrap();

        synth.add_font(font, true);
        synth.set_sample_rate(sample_rate);
        synth.set_gain(1.0);

        synth
    };

    let mut next_value = move || {
        let (l, r) = synth.read_next();

        if let Ok(midi_event) = rx.try_recv() {
            let e = MidiEvent::try_into(midi_event).expect("Fatal error on MidiEvent processing");
            synth.send_event(e).ok();
        }

        (l, r)
    };

    let err_fn = |err| error!("an error occurred on stream: {}", err);

    let stream = device
        .build_output_stream(
            config,
            move |data: &mut [T], _| write_data(data, channels, &mut next_value),
            err_fn,
            None,
        )
        .unwrap();
    stream.play().unwrap();
    stream
}

fn write_data<T>(output: &mut [T], channels: usize, next_sample: &mut dyn FnMut() -> (f32, f32))
where
    T: SizedSample + FromSample<f32>,
{
    for frame in output.chunks_mut(channels) {
        let (l, r) = next_sample();

        let l: T = cpal::Sample::from_sample::<f32>(l);
        let r: T = cpal::Sample::from_sample::<f32>(r);

        let channels = [l, r];

        for (id, sample) in frame.iter_mut().enumerate() {
            *sample = channels[id % 2];
        }
    }
}
