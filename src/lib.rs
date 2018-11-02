extern crate bela;
extern crate monome;
extern crate audio_clock;
use std::{thread, time};
use std::sync::mpsc::{channel, Sender, Receiver};

use bela::*;
use monome::*;
use audio_clock::*;

#[derive(Debug)]
enum Message {
    Key((usize, usize)),
    Start,
    Stop,
    TempoChange(f32)
}

pub struct Renderer {
    clock_up: ClockUpdater,
    clock_cons: ClockConsumer,
    receiver: Receiver<Message>,
    tracks: Vec<TrackControl>,
    tempo: f32
}

impl Renderer {
    fn new(width: usize, height: usize, clock_updater: ClockUpdater, clock_consumer: ClockConsumer, receiver: Receiver<Message>) -> Renderer {
        let mut tracks = Vec::<TrackControl>::new();
        for _ in 0..height {
            let t = TrackControl::new(width);
            tracks.push(t);
        }
        Renderer {
            receiver,
            clock_up: clock_updater,
            clock_cons: clock_consumer,
            tracks,
            tempo: 0.
        }
    }
    fn press(&mut self, x: usize, y: usize) {
       self.tracks[y].press(x);
    }
    fn set_tempo(&mut self, new_tempo: f32) {
       self.tempo = new_tempo;
    }
    pub fn render(&mut self, context: &mut Context) {
        match self.receiver.try_recv() {
            Ok(msg) => {
                match msg {
                    Message::Key((x,y)) => {
                        self.press(x, y);
                    }
                    Message::Start => {
                    }
                    Message::Stop=> {
                    }
                    Message::TempoChange(tempo)=> {
                        self.set_tempo(tempo);
                    }
                }
            }
            Err(err) => {
                match err {
                    std::sync::mpsc::TryRecvError::Empty => {
                    }
                    std::sync::mpsc::TryRecvError::Disconnected => {
                        println!("disconnected");
                    }
                }
            }
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

pub struct Sequencer {
  tempo: f32,
  width: usize,
  height: usize,
  tracks: Vec<TrackControl>,
  sender: Sender<Message>,
  audio_clock: ClockConsumer,
  monome: Monome,
  grid: Vec<u8>
}

impl Sequencer {
    pub fn new(width: usize, height: usize, tempo: f32) -> (Sequencer, Renderer) {
        let (sender, receiver) = channel::<Message>();

        let (clock_updater, clock_consumer) = audio_clock(tempo, 44100);

        let renderer = Renderer::new(16, 8, clock_updater, clock_consumer.clone(), receiver);

        let mut tracks = Vec::<TrackControl>::new();
        for _ in 0..height {
          let t = TrackControl::new(width);
          tracks.push(t);
        }
        let monome = Monome::new("/prefix".to_string()).unwrap();
        let grid = vec![0 as u8; 128];
        (Sequencer {
            tempo: 120.,
            width,
            height,
            tracks,
            sender,
            audio_clock: clock_consumer,
            monome,
            grid
        }, renderer)
    }

    pub fn set_tempo(&mut self, new_tempo: f32) {
       self.tempo = new_tempo;
       self.sender.send(Message::TempoChange(new_tempo));
    }

    fn press(&mut self, x: usize, y: usize) {
      self.tracks[y].press(x);
      self.sender.send(Message::Key((x,y)));
    }
    pub fn render(&mut self) {
        let now = self.audio_clock.beat();
        let sixteenth = now * 4.;
        let pos_in_pattern = (sixteenth as usize) % self.width;

        self.grid.iter_mut().map(|x| *x = 0).count();

        // draw playhead
        for i in 0..self.height {
            self.grid[i * self.width + pos_in_pattern] = 4;
        }

        // draw pattern
        for i in 0..self.height {
            let steps = self.tracks[i].steps();
            for j in 0..self.width {
                if steps[j] != 0 {
                    self.grid[i * self.width + j] = 15;
                }
            }
        }
        self.monome.set_all_intensity(&self.grid);
    }
    pub fn main_thread_work(&self) {
        // noop
    }
    pub fn poll_input(&mut self) {
        match self.monome.poll() {
            Some(MonomeEvent::GridKey{x, y, direction}) => {
                match direction {
                    KeyDirection::Down => {
                        // self.state_tracker.down(x as usize, y as usize);
                    },
                    KeyDirection::Up => {
                        self.press(x as usize, y as usize);
                    }
                }
            }
            Some(_) => {
                // break;
            }
            None => {
                // break;
            }
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
            steps: vec![0; steps]
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
