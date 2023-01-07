use crate::logln;
use chrono::DateTime;
use futures::future::join_all;
use reqwest::Client;
use rss::{Channel, Item};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time;

const FILE: &str = "state";

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct Playable {
    pub source: Option<String>,
    pub title: Option<String>,
    pub album: Option<String>,
    pub progress: (u64, u64),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RssFeed {
    pub uri: String,
    #[serde(skip)]
    pub channel: Option<Channel>,
}
impl RssFeed {
    pub async fn load(&mut self, client: Arc<Client>) {
        if let Ok(content) = client.get(&self.uri).send().await {
            match content.bytes().await {
                Ok(content) => match Channel::read_from(&content[..]) {
                    Ok(channel) => {
                        logln!("updated channel {}", &channel.title);
                        self.channel.replace(channel);
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
    pub rss_feeds: Arc<Mutex<Vec<RssFeed>>>,

    pub uris: HashMap<String, Playable>,
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
            progress: (new_time, _new_progress),
            source: _,
        }: Playable,
    ) {
        match self.uris.get(&uri) {
            Some(Playable {
                title,
                album,
                source,
                progress: (old_time, _progress),
            }) => {
                if new_time > *old_time {
                    self.uris.insert(
                        uri,
                        Playable {
                            source: source.clone(),
                            title: title.clone(),
                            album: album.clone(),
                            progress: new.progress,
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
        self.uris.get(uri).map(|p| p.progress.1)
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
    dates.1.cmp(&dates.0)
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

fn get_recent_episodes(feeds: &[RssFeed]) -> Vec<(String, Item)> {
    const EP_LIM: usize = 100;
    let mut episodes: Vec<(String, Item)> = feeds
        .iter()
        .filter_map(|feed| {
            if let Some(chan) = &feed.channel {
                let zipped = chan
                    .items
                    .iter()
                    // .map(debug_item)
                    .map(move |item| (chan.title.clone(), item.clone()));
                Some(zipped.take(EP_LIM))
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
    let client = Client::builder().user_agent("007").build();
    match client {
        Ok(client) => {
            let client = Arc::new(client);
            for feed in feeds {
                futs.push(feed.load(client.clone()));
            }
            join_all(futs).await;
        }
        Err(err) => logln!("Failed to init reqwest client: {err}"),
    }
}

pub fn start_refresh_thread(
    rss_feeds: Arc<Mutex<Vec<RssFeed>>>,
    episodes: Arc<Mutex<Vec<(String, Item)>>>,
) {
    let mut update_interval = time::interval(Duration::from_secs(3600));
    tokio::spawn(async move {
        loop {
            update_interval.tick().await;
            let mut feed_copy: Vec<RssFeed> = if let Ok(rss_feeds) = rss_feeds.lock() {
                rss_feeds
                    .iter()
                    .map(|RssFeed { uri, .. }| RssFeed {
                        uri: uri.to_string(),
                        channel: None,
                    })
                    .collect()
            } else {
                continue;
            };
            refresh_feeds(&mut feed_copy).await;

            let eps = get_recent_episodes(&feed_copy);
            match rss_feeds.lock() {
                Ok(mut feeds) => *feeds = feed_copy,
                Err(err) => logln!("Failed to refresh feed: {err}"),
            }

            if let Ok(mut episodes) = episodes.lock() {
                *episodes = eps;
            }
        }
    });
}
