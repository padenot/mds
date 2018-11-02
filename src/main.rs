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

struct Renderer {
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
    fn render(&mut self, context: &mut Context) {
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

struct Sequencer {
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
    fn new(width: usize, height: usize, sender: Sender<Message>, audio_clock: ClockConsumer) -> Sequencer {
        let mut tracks = Vec::<TrackControl>::new();
        for _ in 0..height {
          let t = TrackControl::new(width);
          tracks.push(t);
        }
        let monome = Monome::new("/prefix".to_string()).unwrap();
        let grid = vec![0 as u8; 128];
        Sequencer {
            tempo: 120.,
            width,
            height,
            tracks,
            sender,
            audio_clock,
            monome,
            grid,
        }
    }

    fn set_tempo(&mut self, new_tempo: f32) {
       self.tempo = new_tempo;
       self.sender.send(Message::TempoChange(new_tempo));
    }

    fn press(&mut self, x: usize, y: usize) {
      self.tracks[y].press(x);
      self.sender.send(Message::Key((x,y)));
    }
    fn render(&mut self) {
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
    fn main_thread_work(&self) {
        // noop
    }
    fn poll_input(&mut self) {
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


fn go() -> Result<(), error::Error> {
    let tempo = 128.0;
    let (clock_updater, clock_consumer) = audio_clock(tempo, 44100);

    let mut setup = |_context: &mut Context, _user_data: &mut Renderer| -> Result<(), error::Error> {
        println!("Setting up");
        Ok(())
    };

    let mut cleanup = |_context: &mut Context, _user_data: &mut Renderer| {
        println!("Cleaning up");
    };

    let mut render = |context: &mut Context, renderer: &mut Renderer| {
        renderer.render(context);
    };

    let (sender, receiver) = channel::<Message>();

    let renderer = Renderer::new(16, 8, clock_updater, clock_consumer.clone(), receiver);

    let user_data = AppData::new(renderer, &mut render, Some(&mut setup), Some(&mut cleanup));
    let mut bela_app = Bela::new(user_data);
    let mut settings = InitSettings::default();
    bela_app.init_audio(&mut settings)?;
    bela_app.start_audio()?;

    let mut seq = Sequencer::new(16, 8, sender, clock_consumer);
    seq.set_tempo(tempo);

    while !bela_app.should_stop() {
        seq.main_thread_work();
        seq.poll_input();
        seq.render();

        let refresh = time::Duration::from_millis(33);
        thread::sleep(refresh);
    }
    bela_app.stop_audio();
    bela_app.cleanup_audio();
    Ok(())
}

fn main() {
    go().unwrap();
}
