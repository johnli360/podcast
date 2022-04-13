use gst::{prelude::*, Message};
use gstreamer as gst;
use std::{
    io::{self, Write},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

use std::sync::mpsc;

struct Player {
    /// Our one and only element
    playbin: gst::Element,
    /// Are we in the PLAYING state?
    // playing: bool,
    /// Should we terminate execution?
    // observe: bool,
    thread: Option<JoinHandle<()>>,
    /// Is seeking enabled for this media?
    // seek_enabled: bool,
    /// Have we performed the seek already?
    duration: Option<gst::ClockTime>,
}

// struct Observer{
impl Player {
    fn new() -> Self {
        let playbin = gst::ElementFactory::make("playbin", Some("playbin"))
            .expect("Failed to create playbin element");

        // let bus = playbin.bus().unwrap();
        Player {
            playbin,
            // playing: false,
            // observe: false,
            thread: None,
            // observer:
            // seek_enabled: false,
            duration: None,
        }
    }

    fn queue(&self, uri: &str) {
        self.playbin.set_property("uri", uri);
    }

    fn play(&mut self) {

        if let Err(err)  = self.playbin
            .set_state(gst::State::Playing) {
                eprintln!("Unable to set the playbin to the `Playing` state: {err}");
        }
            // .expect("Unable to set the playbin to the `Playing` state");
        // self.observe = true;
        // self

        let bus = self.playbin.bus().unwrap();
        for msg in bus.iter_timed(gst::ClockTime::NONE) {
            use gst::MessageView;

            match msg.view() {
                MessageView::Eos(..) => break,
                MessageView::Error(err) => {
                    println!(
                        "Error from {:?}: {} ({:?})",
                        err.src().map(|s| s.path_string()),
                        err.error(),
                        err.debug()
                    );
                    break;
                }
                _ => (),
            }
        }
    }
}

/* fn observe(player: Arc<Mutex<Player>>) { */
/* // let player = self; */
/* // thread::spawn(move || { */
/* let bus = player.lock().unwrap().playbin.bus().unwrap(); */
/* while !player.lock().unwrap().observe { */
/* let msg = bus.timed_pop(100 * gst::ClockTime::MSECOND); */

/* match msg { */
/* Some(msg) => { */
/* handle_message(&mut player.lock().unwrap(), &msg); */
/* } */
/* None => { */
/* if self.playing { */
/* let position = self */
/* .playbin */
/* .query_position::<gst::ClockTime>() */
/* .expect("Could not query current position."); */

/* // If we didn't know it yet, query the stream duration */
/* if self.duration == gst::ClockTime::NONE { */
/* self.duration = self.playbin.query_duration(); */
/* } */

/* // Print current position and total duration */
/* print!("\rPosition {} / {}", position, self.duration.display()); */
/* io::stdout().flush().unwrap(); */
/* } */
/* } */
/* } */
/* } */
/* // }); */
/* } */

/* fn bus_loop(bus: gst::Bus) { */
/* loop { */
/* let msg = bus.timed_pop(100 * gst::ClockTime::MSECOND); */

/* match msg { */
/* Some(msg) => { */
/* // handle_message(self, &msg); */
/* } */
/* None => { */
/* if self.playing { */
/* let position = self */
/* .playbin */
/* .query_position::<gst::ClockTime>() */
/* .expect("Could not query current position."); */

/* // If we didn't know it yet, query the stream duration */
/* if self.duration == gst::ClockTime::NONE { */
/* self.duration = self.playbin.query_duration(); */
/* } */

/* // Print current position and total duration */
/* print!("\rPosition {} / {}", position, self.duration.display()); */
/* io::stdout().flush().unwrap(); */
/* } */
/* } */
/* } */
/* } */
/* } */

#[derive(Debug)]
enum Cmd {
    Play,
    Queue(String),
}
enum Event {
    Bus(Message),
    Cmd(Cmd),
}

fn new() -> (JoinHandle<()>, mpsc::Sender<Cmd>) {
    let (tx, rx) = mpsc::channel::<Cmd>();
    // let playbin = gst::ElementFactory::make("playbin", Some("playbin"))
    // .expect("Failed to create playbin element");
    // let bus = playbin.bus().unwrap();
    let handle = thread::spawn(move || {
        let mut player = Player::new();
        // let bus = player.playbin.bus().unwrap();

        // let bus_handle = thread::spawn(move || {
        // for bus
        // });
        for cmd in rx.iter() {
            // for cmd in cmdz.chain(buz) {
            println!("play thread received: {cmd:?}");
            match cmd {
                Cmd::Play => {
                    player.play();
                    // .observe();
                }
                Cmd::Queue(uri) => player.queue(&uri),
            }
        }
        // bus_handle.join().expect("bus handle join failed:(");
    });

    (handle, tx)
}

pub fn play(input: &[String], block: bool) {
    println!("{input:?}");
    let (handle, tx) = new();
    tx.send(Cmd::Queue(input[0].clone())).unwrap();
    println!("post queue");
    tx.send(Cmd::Play).unwrap();
    println!("post play");
    // std::
    // loop {
    // }
    // handle.join().unwrap();
    println!("post join");
    // player.queue(&input[0]);
    // player.play();
    // observe(&mut player);
}

fn handle_message(player: &mut Player, msg: &gst::Message) {
    use gst::MessageView;

    match msg.view() {
        MessageView::Error(err) => {
            println!(
                "Error received from element {:?}: {} ({:?})",
                err.src().map(|s| s.path_string()),
                err.error(),
                err.debug()
            );
            // player.observe = true;
        }
        MessageView::Eos(..) => {
            println!("End-Of-Stream reached.");
            // player.observe = true;
        }
        MessageView::DurationChanged(_) => {
            // The duration has changed, mark the current one as invalid
            player.duration = gst::ClockTime::NONE;
        }
        MessageView::StateChanged(state_changed) => {
            if state_changed
                .src()
                .map(|s| s == player.playbin)
                .unwrap_or(false)
            {
                let new_state = state_changed.current();
                let old_state = state_changed.old();

                println!(
                    "Pipeline state changed from {:?} to {:?}",
                    old_state, new_state
                );

                if new_state == gst::State::Playing {
                    let mut seeking = gst::query::Seeking::new(gst::Format::Time);
                    if player.playbin.query(&mut seeking) {
                        let (seekable, start, end) = seeking.result();
                        // player.seek_enabled = seekable;
                        if seekable {
                            println!("Seeking is ENABLED from {} to {}", start, end)
                        } else {
                            println!("Seeking is DISABLED for this stream.")
                        }
                    } else {
                        eprintln!("Seeking query failed.")
                    }
                }
            }
        }
        _ => (),
    }
}
