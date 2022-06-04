use crossterm::terminal;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fs::File;
use std::io::{BufReader, BufWriter};

const FILE: &str = "state";

#[derive(Serialize, Deserialize, Debug)]
struct Playable {
    // uri: String,
    progress: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct State {
    uris: HashMap<String, Playable>,
    #[serde(default = "VecDeque::new")]
    queue: VecDeque<String>,
    #[serde(default = "new_recent")]
    recent: VecDeque<String>,
}
fn new_recent() -> VecDeque<String> {
    VecDeque::with_capacity(32)
}

impl State {
    pub fn from_disc() -> Result<Self, Box<dyn Error>> {
        let state = if let Ok(file) = File::open(FILE) {
            let reader = BufReader::new(file);
            serde_json::from_reader(reader)?
        } else {
            State {
                recent: new_recent(),
                queue: VecDeque::new(),
                uris: HashMap::new(),
            }
        };
        Ok(state)
    }

    pub fn to_disc(&self) -> Result<(), Box<dyn Error>> {
        let file = File::create(FILE)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &self)?;
        Ok(())
    }

    pub fn insert_playable(&mut self, uri: String, progress: u64) {
        self.uris.insert(uri, Playable { progress });
    }

    pub fn queue(&mut self, uri: &str) {
        self.queue.push_back(uri.to_string());
    }

    pub fn pop_queue(&mut self) -> Option<String> {
        self.queue.pop_front()
    }

    pub fn queue_front(&mut self, uri: &str) {
        self.queue.push_front(uri.to_owned())
    }

    pub fn pop_recent(&mut self) -> Option<String> {
        self.recent.pop_back()
    }


    pub fn push_recent(&mut self, uri: &str) {
        if self.recent.len() == self.recent.capacity() {
            self.recent.pop_front();
        }
        self.recent.push_back(uri.to_string());
    }

    pub fn get_pos(&self, uri: &str) -> Option<u64> {
        self.uris.get(uri).map(|p| p.progress)
    }

    pub fn print_recent(&self) {
        if let Err(err) = terminal::disable_raw_mode() {
            eprintln!("{err}");
        };

        println!("----------- RECENT ------------ ");
        self.recent
            .iter()
            .enumerate()
            .for_each(|(i, uri)| println!("{i}: {uri}"));
        println!("------------------------------- ");

        if let Err(err) = terminal::enable_raw_mode() {
            eprintln!("{err}");
        };
    }

    pub fn print_queue(&self) {
        if let Err(err) = terminal::disable_raw_mode() {
            eprintln!("{err}");
        };

        println!("----------- QUEUE ------------- ");
        self.queue
            .iter()
            .enumerate()
            .for_each(|(i, uri)| println!("{i}: {uri}"));
        println!("------------------------------- ");

        if let Err(err) = terminal::enable_raw_mode() {
            eprintln!("{err}");
        };
    }
}
