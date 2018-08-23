extern crate bela;
extern crate monome;
extern crate audio_clock;
use std::{thread, time};
use std::sync::mpsc::{channel, Sender};

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
    clock: ClockUpdater,
    idx: usize,
}

struct Sequencer {
  tempo: f32,
  width: usize,
  height: usize,
  tracks: Vec<TrackControl>,
  sender: Sender<Message>,
  audio_clock: ClockConsumer,
  last_update: usize
}

impl Sequencer {
    fn new(width: usize, height: usize, sender: Sender<Message>, audio_clock: ClockConsumer) -> Sequencer {
        let mut tracks = Vec::<TrackControl>::new();
        for i in 0..height {
          let t = TrackControl::new(width);
          tracks.push(t);
        }
        Sequencer {
            tempo: 120.,
            width,
            height,
            tracks,
            sender,
            audio_clock,
            last_update: 0
        }
    }

    fn set_tempo(&mut self, new_tempo: f32) {
       self.tempo = new_tempo;
       self.sender.send(Message::TempoChange(new_tempo));
    }

    fn press(&mut self, x: usize, y: usize) {
      println!("keydown {} {}", x, y);
      self.tracks[y].press(x);
      self.sender.send(Message::Key((x,y)));
    }
    fn update(&mut self, grid: &mut Vec<u8>) {
      let now = self.audio_clock.beat();
      let sixteenth = now * 4.;
      let pos_in_pattern = (sixteenth as usize) % self.width;

      // draw playhead
      for i in 0..self.height {
          grid[i * self.width + pos_in_pattern] = 4;
      }

      println!("========");
      // draw pattern
      for i in 0..self.height {
          println!("{:?}", self.tracks[i]);
          let steps = self.tracks[i].steps();
          for j in 0..self.width {
              if steps[j] != 0 {
                  grid[i * self.width + j] = 15;
              }
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
    let mut setup = |_context: &mut Context, _user_data: &mut Renderer| -> Result<(), error::Error> {
        println!("Setting up");
        Ok(())
    };

    let mut cleanup = |_context: &mut Context, _user_data: &mut Renderer| {
        println!("Cleaning up");
    };

    let tempo = 128.0;
    let (clock_updater, clock_consumer) = audio_clock(tempo, 44100);

    // Generates a non-bandlimited sawtooth at 110Hz.
    let mut render = |context: &mut Context, renderer: &mut Renderer| {
        renderer.clock.increment(context.audio_frames());
        for (_, samp) in context.audio_out().iter_mut().enumerate() {
            let gain = 0.1;
            *samp = 2. * (renderer.idx as f32 * 110. / 44100.) - 1.;
            *samp *= gain;
            renderer.idx += 1;
            if renderer.idx as f32 > 44100. / 110. {
                renderer.idx = 0;
            }
        }
    };

    let renderer = Renderer {
        clock: clock_updater,
        idx: 0
    };

    let mut monome = Monome::new("/prefix".to_string()).unwrap();
    let user_data = AppData::new(renderer, &mut render, Some(&mut setup), Some(&mut cleanup));
    let mut bela_app = Bela::new(user_data);
    let mut settings = InitSettings::default();
    bela_app.init_audio(&mut settings)?;
    bela_app.start_audio()?;

    let mut x = 0;
    let mut y = 0;
    let mut i = 1;

    let (sender, receiver) = channel::<Message>();
    let mut seq = Sequencer::new(16, 8, sender, clock_consumer);
    seq.set_tempo(tempo);

    let mut grid = vec![0 as u8; 128];

    while !bela_app.should_stop() {
        grid.iter_mut().map(|x| *x = 0).count();
        match monome.poll() {
            Some(MonomeEvent::GridKey{x, y, direction}) => {
                match direction {
                    KeyDirection::Down => {
                       // self.state_tracker.down(x as usize, y as usize);
                    },
                    KeyDirection::Up => {
                        seq.press(x as usize, y as usize);
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
        seq.update(&mut grid);

        println!("{:?}", grid);
        monome.set_all_intensity(&grid);

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
