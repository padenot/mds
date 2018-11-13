extern crate bela;
extern crate mds;
extern crate monome;
extern crate mbms_traits;

use bela::*;
use mds::*;
use monome::*;
use mbms_traits::*;
use std::{thread, time};

fn main() {
    let tempo = 128.0;

    let mut setup =
        |_context: &mut Context, _user_data: &mut MDSRenderer| -> Result<(), error::Error> {
            println!("Setting up");
            Ok(())
        };

    let mut cleanup = |_context: &mut Context, _user_data: &mut MDSRenderer| {
        println!("Cleaning up");
    };

    let mut render = |context: &mut Context, renderer: &mut MDSRenderer| {
        renderer.render(context);
    };

    let (mut seq, renderer) = MDS::new(16, 8, tempo);
    seq.set_tempo(tempo);


    let mut monome = Monome::new("/prefix".to_string()).unwrap();

    let user_data = AppData::new(renderer, &mut render, Some(&mut setup), Some(&mut cleanup));
    let mut bela_app = Bela::new(user_data);
    let mut settings = InitSettings::default();
    bela_app.init_audio(&mut settings);
    bela_app.start_audio();
    let mut grid = [0 as u8; 128];

    while !bela_app.should_stop() {
        let event = monome.poll();
        match event {
            Some(e) => {
                seq.input(e);
            }
            _ => {
                println!("nothing.");
            }
        }
        seq.main_thread_work();
        seq.render(&mut grid);

        monome.set_all_intensity(&grid.to_vec());

        grid.iter_mut().map(|x| *x = 0).count();

        let refresh = time::Duration::from_millis(33);
        thread::sleep(refresh);
    }
    bela_app.stop_audio();
    bela_app.cleanup_audio();
}
