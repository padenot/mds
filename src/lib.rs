extern crate audio_clock;
extern crate bela;
extern crate monome;
extern crate mbms_traits;

use std::sync::mpsc::{channel, Receiver, Sender};
use std::{thread, time};

use audio_clock::*;
use bela::*;
use monome::{MonomeEvent, KeyDirection};
use mbms_traits::{InstrumentRenderer, InstrumentControl};

#[derive(Debug)]
enum Message {
    Key((usize, usize)),
    Start,
    Stop,
    TempoChange(f32),
}

pub struct MDSRenderer {
    clock_up: ClockUpdater,
    clock_cons: ClockConsumer,
    receiver: Receiver<Message>,
    tracks: Vec<TrackControl>,
    tempo: f32,
}

impl MDSRenderer {
    fn new(
        width: usize,
        height: usize,
        clock_updater: ClockUpdater,
        clock_consumer: ClockConsumer,
        receiver: Receiver<Message>,
    ) -> MDSRenderer {
        let mut tracks = Vec::<TrackControl>::new();
        for _ in 0..height {
            let t = TrackControl::new(width);
            tracks.push(t);
        }
        MDSRenderer {
            receiver,
            clock_up: clock_updater,
            clock_cons: clock_consumer,
            tracks,
            tempo: 0.,
        }
    }
    fn press(&mut self, x: usize, y: usize) {
        self.tracks[y].press(x);
    }
    fn set_tempo(&mut self, new_tempo: f32) {
        self.tempo = new_tempo;
    }
}

impl InstrumentRenderer for MDSRenderer {
    fn render(&mut self, context: &mut Context) {
        match self.receiver.try_recv() {
            Ok(msg) => match msg {
                Message::Key((x, y)) => {
                    self.press(x, y);
                }
                Message::Start => {}
                Message::Stop => {}
                Message::TempoChange(tempo) => {
                    self.set_tempo(tempo);
                }
            },
            Err(err) => match err {
                std::sync::mpsc::TryRecvError::Empty => {}
                std::sync::mpsc::TryRecvError::Disconnected => {
                    println!("disconnected");
                }
            },
        }

        let beat = self.clock_cons.beat();
        let sixteenth = beat * 4.;
        let trigger_duration = 0.01; // 10ms
        let integer_sixteenth = sixteenth as usize;
        let analog_frames = context.analog_frames();
        let analog_channels = context.analog_out_channels();
        let audio_frames = context.audio_frames();
        let analog_out = context.analog_out();

        for frame in 0..analog_frames {
            for i in 0..self.tracks.len() {
                let s = self.tracks[i].steps();
                let pos_in_pattern = integer_sixteenth % s.len();
                if s[pos_in_pattern] != 0 && sixteenth.fract() < trigger_duration {
                    analog_out[frame * analog_channels + i] = 1.0;
                } else {
                    analog_out[frame * analog_channels + i] = 0.0;
                }
            }
        }

        self.clock_up.increment(audio_frames);
    }
}

pub struct MDS {
    tempo: f32,
    width: usize,
    height: usize,
    tracks: Vec<TrackControl>,
    sender: Sender<Message>,
    audio_clock: ClockConsumer,
    grid: Vec<u8>,
}

impl MDS {
    pub fn new(width: usize, height: usize, tempo: f32) -> (MDS, MDSRenderer) {
        let (sender, receiver) = channel::<Message>();

        let (clock_updater, clock_consumer) = audio_clock(tempo, 44100);

        let renderer = MDSRenderer::new(16, 8, clock_updater, clock_consumer.clone(), receiver);

        let mut tracks = Vec::<TrackControl>::new();
        for _ in 0..height {
            let t = TrackControl::new(width);
            tracks.push(t);
        }
        let grid = vec![0 as u8; 128];
        (
            MDS {
                tempo: 120.,
                width,
                height,
                tracks,
                sender,
                audio_clock: clock_consumer,
                grid,
            },
            renderer,
        )
    }

    pub fn set_tempo(&mut self, new_tempo: f32) {
        self.tempo = new_tempo;
        self.sender.send(Message::TempoChange(new_tempo));
    }

    fn press(&mut self, x: usize, y: usize) {
        self.tracks[y].press(x);
        self.sender.send(Message::Key((x, y)));
    }
}

impl InstrumentControl for MDS {
    fn render(&mut self, grid: &mut [u8; 128]) {
        let now = self.audio_clock.beat();
        let sixteenth = now * 4.;
        let pos_in_pattern = (sixteenth as usize) % self.width;

        grid.iter_mut().map(|x| *x = 0).count();

        // draw playhead
        for i in 0..self.height {
            grid[i * self.width + pos_in_pattern] = 4;
        }

        // draw pattern
        for i in 0..self.height {
            let steps = self.tracks[i].steps();
            for j in 0..self.width {
                if steps[j] != 0 {
                    grid[i * self.width + j] = 15;
                }
            }
        }
    }
    fn main_thread_work(&mut self) {
        // noop
    }
    fn input(&mut self, event: MonomeEvent) {
        match event {
            MonomeEvent::GridKey { x, y, direction } => {
                match direction {
                    KeyDirection::Down => {
                        // self.state_tracker.down(x as usize, y as usize);
                    }
                    KeyDirection::Up => {
                        self.press(x as usize, y as usize);
                    }
                }
            }
            _ => {  }
        }
    }
}

// main thread
#[derive(Debug)]
struct TrackControl {
    steps: Vec<u8>,
}

impl TrackControl {
    fn new(steps: usize) -> TrackControl {
        TrackControl {
            steps: vec![0; steps],
        }
    }
    fn press(&mut self, x: usize) {
        if self.steps[x] == 0 {
            self.steps[x] = 1;
        } else {
            self.steps[x] = 0;
        }
    }
    fn steps(&self) -> &[u8] {
        &self.steps
    }
}
