extern crate mds;
extern crate bela;
use mds::*;
use bela::*;
use std::{thread, time};

fn main() {
    let tempo = 128.0;

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

    let (mut seq, renderer) = Sequencer::new(16, 8, tempo);
    seq.set_tempo(tempo);

    let user_data = AppData::new(renderer, &mut render, Some(&mut setup), Some(&mut cleanup));
    let mut bela_app = Bela::new(user_data);
    let mut settings = InitSettings::default();
    bela_app.init_audio(&mut settings);
    bela_app.start_audio();


    while !bela_app.should_stop() {
        seq.main_thread_work();
        seq.poll_input();
        seq.render();

        let refresh = time::Duration::from_millis(33);
        thread::sleep(refresh);
    }
    bela_app.stop_audio();
    bela_app.cleanup_audio();
}
