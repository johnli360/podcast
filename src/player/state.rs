use crate::logln;
use chrono::DateTime;
use gstreamer::ClockTime;
use reqwest::Client;
use rss::{Channel, Item};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::error::Error;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc::{self, channel};
use tokio::{select, time};

const FILE: &str = "state";

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct Playable {
    pub source: Option<String>,
    pub title: Option<String>,
    pub album: Option<String>,
    pub updated: Option<u64>,
    pub progress: Option<u64>,
    pub length: Option<u64>,
}
impl Playable {
    pub fn progress_string(&self) -> String {
        if let Some(p) = self.progress.map(ClockTime::from_seconds) {
            if let Some(length) = self.length {
                let f = (100 * self.progress.unwrap())
                    .checked_div(1 + length)
                    .unwrap();
                return format!("{f}%");
            } else {
                return format!("{}m", p.minutes());
            }
        }
        "n/a".into()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Episode {
    pub channel_title: String,
    pub item: Item,
}

impl Eq for Episode {}

impl Ord for Episode {
    fn cmp(&self, other: &Self) -> Ordering {
        self.cmp_date(other)
    }
}

impl PartialOrd for Episode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Episode {
    fn cmp_date(&self, other: &Episode) -> Ordering {
        //TODO: don't want to parse all this stuff for every compare
        let dates = (
            self.item
                .pub_date()
                .map(DateTime::parse_from_rfc2822)
                .map(Result::ok),
            other
                .item
                .pub_date()
                .map(DateTime::parse_from_rfc2822)
                .map(Result::ok),
        );
        dates.1.cmp(&dates.0)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RssFeed {
    pub uri: String,
    #[serde(skip)]
    pub channel: Arc<RwLock<Option<Channel>>>,
}
impl RssFeed {
    pub async fn load(&self, client: &Client) {
        if let Ok(content) = client.get(&self.uri).send().await {
            match content.bytes().await {
                Ok(content) => match Channel::read_from(&content[..]) {
                    Ok(channel) => {
                        logln!("updated channel {}", &channel.title);
                        match self.channel.write() {
                            Ok(mut guard) => {
                                let _old = guard.replace(channel);
                                //TODO: return old and use it to diff ?
                            }
                            Err(err) => logln!("failed to lock channel {err}"),
                        }
                    }
                    Err(err) => logln!("failed to read channel {} - {err}", self.uri),
                },
                Err(err) => logln!("failed to update {} - {err}", self.uri),
            }
        }
    }
}

pub fn get_time() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Err(err) => {
            logln!("failed to get time: {err}");
            0
        }
        Ok(t) => t.as_secs(),
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct State {
    #[serde(default = "new_rss_feeds")]
    pub rss_feeds: Mutex<Vec<Arc<RssFeed>>>,

    pub uris: HashMap<String, Playable>,
    #[serde(default = "VecDeque::new")]
    pub queue: VecDeque<String>,
    #[serde(default = "new_recent")]
    pub recent: VecDeque<String>,
}

fn new_rss_feeds() -> Mutex<Vec<Arc<RssFeed>>> {
    Mutex::new(Vec::new())
}

fn new_recent() -> VecDeque<String> {
    VecDeque::with_capacity(32)
}

impl State {
    pub fn from_disc2(file: &str) -> Result<Self, Box<dyn Error>> {
        let mut state = if let Ok(file) = File::open(file) {
            let reader = BufReader::new(file);
            serde_json::from_reader(reader)?
        } else {
            State {
                rss_feeds: new_rss_feeds(),
                recent: new_recent(),
                queue: VecDeque::new(),
                uris: HashMap::new(),
            }
        };
        state.recent.reserve(32);
        Ok(state)
    }

    pub fn from_disc() -> Result<Self, Box<dyn Error>> {
        Self::from_disc2(FILE)
    }

    pub fn to_disc(&self) -> Result<(), Box<dyn Error>> {
        let file = File::create(FILE)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &self)?;
        Ok(())
    }

    pub fn insert_playable(&mut self, uri: String, playable: Playable) {
        self.uris.insert(uri, playable);
    }

    pub fn update_playable(
        &mut self,
        uri: String,
        new @ Playable {
            title: _,
            album: _,
            progress: _,
            length: _,
            source: _,
            updated: new_time,
        }: Playable,
    ) {
        match self.uris.get(&uri) {
            Some(old) => {
                if new_time > old.updated {
                    self.uris.insert(
                        uri,
                        Playable {
                            source: old.source.clone(),
                            title: old.title.clone(),
                            album: old.album.clone(),
                            progress: new.progress,
                            updated: new_time,
                            length: new.length,
                        },
                    );
                }
            }
            None => {
                self.uris.insert(uri, new);
            }
        }
    }

    pub fn reset_pos(&mut self, uri: &str) {
        self.uris.remove(uri);
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
        self.recent.pop_front()
    }

    pub fn push_recent(&mut self, uri: &str) {
        if self.recent.len() == self.recent.capacity() {
            self.recent.pop_back();
        }
        self.recent.push_front(uri.to_string());
    }

    pub fn get_pos(&self, uri: &str) -> Option<u64> {
        self.uris.get(uri).and_then(|p| p.progress)
    }
}

#[allow(dead_code)]
fn debug_item(item: &Item) -> &Item {
    use std::io::Write;
    let mut file = File::options()
        .create(true)
        .append(true)
        .open("debug_file")
        .unwrap();
    writeln!(&mut file, "comments: {:?}", item.comments).unwrap();
    writeln!(&mut file, "enclosure: {:?}", item.enclosure).unwrap();
    writeln!(&mut file, "description: {:?}", item.description).unwrap();
    writeln!(&mut file, "\n\n\n\n\n").unwrap();
    item
}

pub fn start_refresh_thread(episodes: Arc<Mutex<BTreeSet<Episode>>>) -> mpsc::Sender<Arc<RssFeed>> {
    let (feed_tx, mut feed_rx) = channel::<Arc<RssFeed>>(10);
    tokio::spawn(async move {
        let (ep_tx, mut ep_rx) = channel::<Episode>(10);
        loop {
            select! {
                Some(feed) = feed_rx.recv() => {
                    observe_feed(feed, ep_tx.clone());
                }
                Some(ep) = ep_rx.recv() => {
                    match episodes.lock() {
                        Ok(mut episodes) => { episodes.insert(ep); },
                        Err(err) => logln!("{err}"),
                    }
                }
            }
        }
    });
    feed_tx
}

fn observe_feed(feed: Arc<RssFeed>, tx: mpsc::Sender<Episode>) {
    tokio::spawn(async move {
        let mut update_interval = time::interval(Duration::from_secs(3600));
        let client = Client::builder().user_agent("007").build();
        match client {
            Ok(client) => {
                let mut new_episodes: Vec<Episode> = Vec::new();
                loop {
                    update_interval.tick().await;
                    feed.load(&client).await;
                    if let Ok(Some(channel)) = feed.channel.read().as_deref() {
                        let channel_title = channel.title();
                        for e in &channel.items {
                            let ep = Episode {
                                channel_title: channel_title.to_string(),
                                item: e.clone(),
                            };
                            new_episodes.push(ep);
                        }
                    }

                    while let Some(ep) = new_episodes.pop() {
                        if let Err(err) = tx.send(ep).await {
                            logln!("failed to send ep: {err}")
                        }
                    }
                }
            }
            Err(err) => logln!("Failed to init reqwest client: {err}"),
        }
    });
}
