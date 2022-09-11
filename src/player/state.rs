use chrono::DateTime;
use futures::future::join_all;
use rss::{Channel, Item};
use serde::{Deserialize, Serialize};
use std::cmp::{min, Ordering};
use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time;
use crate::ui::_log;

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
    #[serde(default = "new_rss_feeds")]
    pub rss_feeds: Arc<Mutex<Vec<RssFeed>>>,

    uris: HashMap<String, Playable>,
    #[serde(default = "VecDeque::new")]
    pub queue: VecDeque<String>,
    #[serde(default = "new_recent")]
    pub recent: VecDeque<String>,
}

fn new_rss_feeds() -> Arc<Mutex<Vec<RssFeed>>> {
    Arc::new(Mutex::new(Vec::new()))
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
                rss_feeds: new_rss_feeds(),
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

fn get_recent_episodes(feeds: &Vec<RssFeed>) -> Vec<(String, Item)> {
    let mut episodes: Vec<(String, Item)> = feeds
        .into_iter()
        .filter_map(|feed| {
            if let Some(chan) = &feed.channel {
                let bound = min(chan.items.len(), 50);
                let zipped = chan
                    .items
                    .iter()
                    .map(move |item| (chan.title.clone(), item.clone()));
                Some(zipped.take(bound))
            } else {
                None
            }
        })
        .flatten()
        .collect();
    episodes.sort_by(|(t1, i1), (t2, i2)| cmp_date(&(t1, i1), &(t2, i2)));
    episodes
}

async fn refresh_feeds(feeds: &mut Vec<RssFeed>) {
    let mut futs = Vec::with_capacity(feeds.len());
    for feed in feeds {
        futs.push(feed.load());
    }
    join_all(futs).await;
}

pub fn start_refresh_thread(
    rss_feeds: Arc<Mutex<Vec<RssFeed>>>,
    episodes: Arc<Mutex<Vec<(String, Item)>>>,
) {
    let mut update_interval = time::interval(Duration::from_millis(10_000));
    tokio::spawn(async move {
        loop {
            update_interval.tick().await;
            let mut feed_copy = if let Ok(rss_feeds) = rss_feeds.lock() {
                rss_feeds.clone()
            } else {
                continue;
            };
            refresh_feeds(&mut feed_copy).await;
            let eps = get_recent_episodes(&feed_copy);
            if let Ok(mut episodes) = episodes.lock() {
                *episodes = eps;
            }
        }
    });
}
