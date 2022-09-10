use chrono::DateTime;
use futures::future::join_all;
use rss::{Channel, Item};
use serde::{Deserialize, Serialize};
use std::cmp::{min, Ordering};
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RssFeed {
    pub uri: String,
    #[serde(skip)]
    pub channel: Option<Channel>,
}
impl RssFeed {
    pub async fn load(&mut self) {
        if let Ok(content) = reqwest::get(&self.uri).await {
            if let Ok(content) = content.bytes().await {
                if let Ok(channel) = Channel::read_from(&content[..]) {
                    self.channel.replace(channel);
                }
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct State {
    #[serde(default = "Vec::new")]
    pub rss_feeds: Vec<RssFeed>,

    uris: HashMap<String, Playable>,
    #[serde(default = "VecDeque::new")]
    pub queue: VecDeque<String>,
    #[serde(default = "new_recent")]
    pub recent: VecDeque<String>,
}

fn new_recent() -> VecDeque<String> {
    VecDeque::with_capacity(32)
}

impl State {
    pub fn from_disc() -> Result<Self, Box<dyn Error>> {
        let mut state = if let Ok(file) = File::open(FILE) {
            let reader = BufReader::new(file);
            serde_json::from_reader(reader)?
        } else {
            State {
                rss_feeds: Vec::new(),
                recent: new_recent(),
                queue: VecDeque::new(),
                uris: HashMap::new(),
            }
        };
        state.recent.reserve(32);
        Ok(state)
    }

    pub fn to_disc(&self) -> Result<(), Box<dyn Error>> {
        let file = File::create(FILE)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &self)?;
        Ok(())
    }

    pub async fn update_feeds(&mut self) {
        let mut futs = Vec::with_capacity(self.rss_feeds.len());
        for feed in &mut self.rss_feeds {
            futs.push(feed.load());
        }
        join_all(futs).await;
    }

    pub fn get_episodes(&self) -> Vec<(&String, &Item)> {
        let mut episodes: Vec<(&String, &Item)> = self
            .rss_feeds
            .iter()
            .filter_map(|feed| {
                if let Some(chan) = &feed.channel {
                    let bound = min(chan.items.len(), 50);
                    let zipped = chan.items.iter().map(|item| (&chan.title, item));
                    Some(zipped.take(bound))
                } else {
                    None
                }
            })
            .flatten()
            .collect();
        episodes.sort_by(Self::cmp_date);
        return episodes;
    }

    fn cmp_date(date1: &(&String, &Item), date2: &(&String, &Item)) -> Ordering {
        let dates = (
            date1
                .1
                .pub_date()
                .map(DateTime::parse_from_rfc2822)
                .map(Result::ok),
            date2
                .1
                .pub_date()
                .map(DateTime::parse_from_rfc2822)
                .map(Result::ok),
        );
        return dates.1.cmp(&dates.0);
    }

    pub fn insert_playable(&mut self, uri: String, progress: u64) {
        self.uris.insert(uri, Playable { progress });
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
        self.uris.get(uri).map(|p| p.progress)
    }
}
