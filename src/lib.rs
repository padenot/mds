extern crate audio_clock;
extern crate bela;
extern crate euclidian_rythms;
extern crate mbms_traits;
extern crate monome;
extern crate smallvec;

use std::cmp;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::{thread, time};

use audio_clock::*;
use bela::*;
use euclidian_rythms::*;
use mbms_traits::*;
use monome::{KeyDirection, MonomeEvent};
use smallvec::SmallVec;

#[derive(Debug)]
enum Message {
    Key((usize, usize)),
    Start,
    Stop,
    TempoChange(f32),
    Euclidian((usize, usize, usize)),
    Loop((usize, usize, usize)),
}

pub struct MDSRenderer {
    clock_updater: ClockUpdater,
    clock_consumer: ClockConsumer,
    receiver: Receiver<Message>,
    tracks: Vec<TrackControl>,
    tempo: f32,
    port_range: (BelaPort, BelaPort),
}

impl MDSRenderer {
    fn new(
        width: usize,
        height: usize,
        clock_updater: ClockUpdater,
        clock_consumer: ClockConsumer,
        receiver: Receiver<Message>,
        port_range: (BelaPort, BelaPort),
    ) -> MDSRenderer {
        let mut tracks = Vec::<TrackControl>::new();
        for _ in 0..height {
            let t = TrackControl::new(width);
            tracks.push(t);
        }
        MDSRenderer {
            receiver,
            clock_updater,
            clock_consumer,
            tracks,
            tempo: 0.,
            port_range,
        }
    }
    fn press(&mut self, x: usize, track_idx: usize) {
        self.tracks[track_idx].press(x);
    }
    fn set_tempo(&mut self, new_tempo: f32) {
        self.tempo = new_tempo;
    }
    fn euclidian(&mut self, track_idx: usize, steps: usize, pulse: usize) {
        self.tracks[track_idx].euclidian(steps, pulse);
    }
}

impl InstrumentRenderer for MDSRenderer {
    fn render(&mut self, context: &mut Context) {
        match self.receiver.try_recv() {
            Ok(msg) => match msg {
                Message::Key((x, track_idx)) => {
                    self.press(x, track_idx);
                }
                Message::Start => {}
                Message::Stop => {}
                Message::TempoChange(tempo) => {
                    self.set_tempo(tempo);
                }
                Message::Euclidian((track_idx, steps, pulses)) => {
                    self.euclidian(track_idx, steps, pulses);
                }
                Message::Loop((track, start, end)) => {
                    // ...
                }
            },
            Err(err) => match err {
                std::sync::mpsc::TryRecvError::Empty => {}
                std::sync::mpsc::TryRecvError::Disconnected => {
                    println!("disconnected");
                }
            },
        }

        let frames = context.audio_frames();
        let beat = self.clock_consumer.beat();
        let sixteenth = beat * 4.;
        let trigger_duration = 0.01; // 10ms
        let integer_sixteenth = sixteenth as usize;
        let analog_frames = context.analog_frames();
        let mut ssteps = [0 as u8; 16];

        match self.port_range {
            (BelaPort::Digital(start), BelaPort::Digital(end)) => {
                let digital_frames = context.digital_frames();
                for frame in 0..digital_frames {
                    for i in 0..self.tracks.len() {
                        self.tracks[i].steps(beat, &mut ssteps);
                        let pos_in_pattern = integer_sixteenth % 16;
                        if ssteps[pos_in_pattern] != 0 && sixteenth.fract() < trigger_duration {
                            assert!(start + i <= end);
                            context.digital_write_once(frame, start + i, 1);
                        } else {
                            context.digital_write_once(frame, start + i, 0);
                        }
                    }
                }
            }
            (BelaPort::AnalogOut(start), BelaPort::AnalogOut(end)) => {
                let analog_channels = context.analog_out_channels();
                let analog_out = context.analog_out();
                for frame in 0..analog_frames {
                    for i in 0..self.tracks.len() {
                        let s = self.tracks[i].steps(beat, &mut ssteps);
                        let pos_in_pattern = integer_sixteenth % 16;
                        if ssteps[pos_in_pattern] != 0 && sixteenth.fract() < trigger_duration {
                            analog_out[frame * analog_channels + i + start] = 1.0;
                        } else {
                            analog_out[frame * analog_channels + i + start] = 0.0;
                        }
                    }
                }
            }
            _ => panic!("bad bad bad"),
        }
        self.clock_updater.increment(frames);
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
    state_tracker: GridStateTracker,
}

impl MDS {
    pub fn new(
        ports: (BelaPort, BelaPort),
        width: usize,
        height: usize,
        tempo: f32,
    ) -> (MDS, MDSRenderer) {
        let (sender, receiver) = channel::<Message>();

        let (clock_updater, clock_consumer) = audio_clock(tempo, 44100);

        let portrange = match ports {
            (BelaPort::Digital(start), BelaPort::Digital(end)) => {
                if end - start != height {
                    panic!("not enought output ports");
                }
            }
            (BelaPort::AnalogOut(start), BelaPort::AnalogOut(end)) => {
                if end - start != height {
                    panic!("not enought output ports");
                }
            }
            _ => {
                panic!("bad BelaPort for MDS");
            }
        };

        let renderer = MDSRenderer::new(
            16,
            8,
            clock_updater,
            clock_consumer.clone(),
            receiver,
            ports,
        );
        let state_tracker = GridStateTracker::new(16, 8);

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
                state_tracker,
            },
            renderer,
        )
    }

    pub fn set_tempo(&mut self, new_tempo: f32) {
        self.tempo = new_tempo;
        self.sender.send(Message::TempoChange(new_tempo));
    }

    fn press(&mut self, x: usize, track_idx: usize) {
        self.tracks[track_idx].press(x);
        self.sender.send(Message::Key((x, track_idx)));
    }
    fn euclidian(&mut self, track: usize, steps: usize, pulses: usize) {
        self.tracks[track].euclidian(steps, pulses);
        self.sender.send(Message::Euclidian((track, steps, pulses)));
    }
    fn looop(&mut self, track: usize, start: usize, end: usize) {
        self.tracks[track].looop(start, end);
        self.sender.send(Message::Loop((track, start, end)));
    }
}

#[derive(Clone, PartialEq)]
enum MDSIntent {
    Nothing,
    Tick,
    Euclidian,
    Loop,
}

#[derive(Debug, Copy, Clone)]
enum MDSAction {
    Nothing,
    Tick((usize, usize)),
    Euclidian((usize, usize, usize)),
    Loop((usize, usize, usize)),
}

struct GridStateTracker {
    buttons: Vec<MDSIntent>,
    width: usize,
    height: usize,
}

impl GridStateTracker {
    fn new(width: usize, height: usize) -> GridStateTracker {
        GridStateTracker {
            width,
            height,
            buttons: vec![MDSIntent::Nothing; width * height],
        }
    }

    fn down(&mut self, x: usize, y: usize) {
        if y == 0 {
            // control row, does nothing for now.
            self.buttons[Self::idx(self.width, x, y)] = MDSIntent::Tick;
        } else {
            // track rows
            // If, when pressing down, we find another button on the same line already down, if the
            // first button is on the left, this is an euclidian rythm pattern. Otherwise, it's a
            // loop for the current pattern.
            let mut foundanother = false;
            for i in 0..self.width {
                if self.buttons[Self::idx(self.width, i, y)] != MDSIntent::Nothing {
                    if i < x {
                        self.buttons[Self::idx(self.width, i, y)] = MDSIntent::Euclidian;
                        self.buttons[Self::idx(self.width, x, y)] = MDSIntent::Euclidian;
                        foundanother = true;
                        break;
                    } else {
                        self.buttons[Self::idx(self.width, i, y)] = MDSIntent::Loop;
                        self.buttons[Self::idx(self.width, x, y)] = MDSIntent::Loop;
                        foundanother = true;
                        break;
                    }
                }
            }
            if !foundanother {
                self.buttons[Self::idx(self.width, x, y)] = MDSIntent::Tick;
            }
        }
    }
    fn up(&mut self, x: usize, y: usize) -> MDSAction {
        if y == 0 {
            // control row, nothing for now
            MDSAction::Nothing
        } else {
            match self.buttons[Self::idx(self.width, x, y)].clone() {
                MDSIntent::Nothing => {
                    // !? pressed a key during startup
                    MDSAction::Nothing
                }
                MDSIntent::Tick => {
                    self.buttons[Self::idx(self.width, x, y)] = MDSIntent::Nothing;
                    MDSAction::Tick((x, y - 1))
                }
                MDSIntent::Euclidian => {
                    // Find the other button that is down, if we find one that is also euclidian
                    // loop between the two points. Otherwise, it's just the second button of the
                    // euclidian pattern command that is being released.
                    let mut other: Option<usize> = None;
                    for i in 0..self.width {
                        if i != x
                            && self.buttons[Self::idx(self.width, i, y)] == MDSIntent::Euclidian
                        {
                            other = Some(i);
                        }
                    }

                    self.buttons[Self::idx(self.width, x, y)] = MDSIntent::Nothing;

                    // The bigger number is the number of steps, the smaller the number of pulses.
                    match other {
                        Some(i) => {
                            let pulses = std::cmp::min(x, i);
                            let steps = std::cmp::max(x, i);
                            MDSAction::Euclidian((y - 1, steps + 1, pulses + 1))
                        }
                        None => MDSAction::Nothing,
                    }
                }
                MDSIntent::Loop => {
                    // Find the other button that is down, if we find one that is also a loop,
                    // loop between the two points. Otherwise, it's just the second loop point that is
                    // being released.
                    let mut other: Option<usize> = None;
                    for i in 0..self.width {
                        if i != x && self.buttons[Self::idx(self.width, i, y)] == MDSIntent::Loop {
                            other = Some(i);
                        }
                    }

                    self.buttons[Self::idx(self.width, x, y)] = MDSIntent::Nothing;

                    match other {
                        Some(i) => {
                            let start = std::cmp::min(x, i);
                            let end = std::cmp::max(y, i);
                            MDSAction::Loop((y - 1, start, end))
                        }
                        None => MDSAction::Nothing,
                    }
                }
            }
        }
    }
    fn idx(width: usize, x: usize, y: usize) -> usize {
        y * width + x
    }
}

impl InstrumentControl for MDS {
    fn render(&mut self, grid: &mut [u8; 128]) {
        let now = self.audio_clock.beat();
        let sixteenth = now * 4.;
        let mut steps = [0 as u8; 16];
        let pos_in_pattern = (sixteenth as usize) % self.width;

        grid.iter_mut().map(|x| *x = 0).count();

        // draw playhead
        for i in 1..self.height + 1 {
            grid[i * self.width + pos_in_pattern] = 4;
        }

        // draw pattern
        for i in 0..self.height {
            self.tracks[i].steps(now, &mut steps);
            for j in 0..self.width {
                if steps[j] != 0 {
                    grid[(1 + i) * self.width + j] = 15;
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
                        self.state_tracker.down(x as usize, y as usize);
                    }
                    KeyDirection::Up => {
                        match self.state_tracker.up(x as usize, y as usize) {
                            MDSAction::Nothing => {
                                // nothing
                            }
                            MDSAction::Tick((x, track_idx)) => {
                                self.press(x, track_idx);
                            }
                            MDSAction::Euclidian((track_idx, steps, pulses)) => {
                                self.euclidian(track_idx, steps, pulses);
                            }
                            MDSAction::Loop((track_idx, start, end)) => {
                                self.looop(track_idx, start, end);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

// main thread
#[derive(Debug)]
struct TrackControl {
    steps: SmallVec<[u8; 16]>,
}

impl TrackControl {
    fn new(steps: usize) -> TrackControl {
        let mut steps = SmallVec::<[u8; 16]>::new();
        steps.resize(16, 0);
        TrackControl { steps }
    }
    fn press(&mut self, x: usize) {
        // if press when in euclidian, freeze the euclidian pattern as if it was computed for time
        // 0 and add the press (activating or deactivating a step). It would be better to freeze
        // the current pattern.
        if self.steps.len() != 16 {
            let mut current_steps = [0 as u8; 16];
            self.steps(0., &mut current_steps);
            self.steps.resize(16, 0);
            for i in 0..16 {
                self.steps[i] = current_steps[i];
            }
        }
        if self.steps[x] == 0 {
            self.steps[x] = 1;
        } else {
            self.steps[x] = 0;
        }
    }
    fn euclidian(&mut self, steps: usize, pulses: usize) {
        self.steps.resize(steps, 0);
        self.steps.iter_mut().map(|x| *x = 0).count();

        euclidian_rythms::euclidian_rythm(&mut self.steps[..steps], pulses);
    }
    fn looop(&mut self, start: usize, end: usize) {
        // ...
    }
    fn steps(&mut self, beat: f32, steps: &mut [u8; 16]) {
        let beat_rounded_bar = (beat as usize) / 4 * 4 * 4;
        let offset = (beat_rounded_bar as usize) % self.steps.len();
        for i in 0..16 {
            steps[i] = self.steps[(offset + i) % self.steps.len()];
        }
    }
}
