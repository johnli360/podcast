use gst::{prelude::*, Bus, Message};
use gstreamer as gst;
use tokio_stream::{Stream, StreamExt};
// use std::futures::StreamExt;
use std::{
    future::Future,
    io::{self, Write},
    pin::Pin,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    task::Poll,
    thread::{self, JoinHandle},
};
use tokio::{select, sync::mpsc::Sender};

use std::sync::mpsc;

struct Player {
    /// Our one and only element
    playbin: gst::Element,
    /// Are we in the PLAYING state?
    // playing: bool,
    /// Should we terminate execution?
    // observe: bool,
    // thread: Option<JoinHandle<()>>,
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
            // thread: None,
            // observer:
            // seek_enabled: false,
            duration: None,
        }
    }

    fn queue(&self, uri: &str) {
        self.playbin.set_property("uri", uri);
    }

    fn play(&mut self) {
        if let Err(err) = self.playbin.set_state(gst::State::Playing) {
            eprintln!("Unable to set the playbin to the `Playing` state: {err}");
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
pub enum Cmd {
    Play,
    Queue(String),
    Shutdown,
}

struct TimedBus {
    bus: Bus,
}
impl Future for TimedBus {
    type Output = Message;

    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        todo!()
    }
}

fn handle_cmd(cmd: Cmd, player: &mut Player) -> bool {
    // if let Some(cmd) = cmd {
    match cmd {
        Cmd::Play => {
            player.play();
        }
        Cmd::Queue(uri) => player.queue(&uri),
        Cmd::Shutdown => {
            // Shutdown pipeline
            player
                .playbin
                .set_state(gst::State::Null)
                .expect("Unable to set the pipeline to the `Null` state");
            return false;
        }
    }
    return true;
    // }
}

async fn new() -> Sender<Cmd> {
    // let (tx, rx) = tokio::channel::<Cmd>();
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    // let handle = thread::spawn(move || {
    // bus_stream.next();
    // bus_stream.

    tokio::spawn(async move {
        let mut player = Player::new();
        let bus = player.playbin.bus().unwrap();
        // bus.add_signal_watch();
        println!("add_watch!");
        // bus.add_watch(|bus, msg| {
        // println!("bus: {msg:?}");
        // Continue(true)
        // })
        // .expect("add_watch error");
        // let mut bus_stream = bus.stream();
        // bus.enable_sync_message_emission();
        // bus.
        let mut bus_stream = bus.stream();
        // while let Some(cmd) = rx.recv().await {
        // println!("loop: {cmd:?}");
        // handle_cmd(cmd, &mut player);
        // }
        'exit: loop {
            // println!("loop");
            // }
            select! {
            Some(cmd) = rx.recv() => {
            println!("new cmd: {cmd:?}");
            if !handle_cmd(cmd, &mut player)  {
                break 'exit
            }
            }
            msg = bus_stream.next() => {
            // println!("new bus msg: {msg:?}");
            if let Some(msg) = msg {
            handle_message(&mut player, &msg)
            }
            }
            }
        }
    });

    // let bus_handle = thread::spawn(move || {
    // for bus
    // });
    /*     for cmd in rx.iter() { */
    /* // for cmd in cmdz.chain(buz) { */
    /* println!("play thread received: {cmd:?}"); */
    /* } */

    tx
}

pub async fn play(input: &str, block: bool) -> Sender<Cmd> {
    println!("{input:?}");
    // let (handle, tx) =
    let tx = new().await;
    if let Err(err) = tx.send(Cmd::Queue(input.to_string())).await {
        eprintln!("queue {err}");
    }
    // println!("post queue");

    if let Err(err) = tx.send(Cmd::Play).await {
        eprintln!("play {err}");
    }
    // println!("post play");
    tx
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
