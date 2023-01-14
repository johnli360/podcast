use crate::logln;
use strum_macros::EnumString;
use strum_macros::{AsStaticStr, Display};

use super::state::Playable;

#[derive(Debug, EnumString, AsStaticStr, Display, PartialEq, Eq)]
#[strum(serialize_all = "snake_case")]
pub enum Cmd {
    Next,
    Prev,
    Play,
    Pause,
    PlayPause,
    Queue(String),
    Shutdown,
    Seek(u64),
    SeekRelative(i64),
    DeleteQueue(usize),
    DeleteRecent(usize),
    Subscribe(String),
    Update(UpdateArgs),
}

#[derive(Eq, PartialEq, Debug)]
pub struct UpdateArgs(pub String, pub Playable);
impl Default for UpdateArgs {
    fn default() -> Self {
        UpdateArgs(
            String::default(),
            Playable {
                title: None,
                album: None,
                source: None,
                progress: (0, 0),
            },
        )
    }
}

impl UpdateArgs {
    pub fn to_cmd_string(self) -> String {
        let Self(
            uri,
            Playable {
                title: _,
                album: _,
                source: _,
                progress: (t, p),
            },
        ) = self;
        format!("update({uri},{t},{p})")
    }

    pub fn parse(raw: &str) -> Option<Self> {
        let mut rs = raw.split(',');
        let uri = rs.next()?;
        let time = rs.next().and_then(|s| s.parse::<u64>().ok())?;
        let progress = rs.next().and_then(|s| s.parse::<u64>().ok())?;
        Some(UpdateArgs(
            uri.to_string(),
            Playable {
                title: None,
                album: None,
                source: None,
                progress: (time, progress),
            },
        ))
    }
}

pub fn parse_cmd(buf: &str) -> Option<Cmd> {
    if let cmd @ Ok(_) = buf.trim_end().parse() {
        cmd.ok()
    } else if let cmd @ Some(_) = parse_cmd_arg(buf) {
        cmd
    } else {
        logln!("parse failed: {buf}");
        None
    }
}

fn parse_cmd_arg(buf: &str) -> Option<Cmd> {
    if let Some((variant, Some(arg))) = buf
        .split_once('(')
        .map(|(variant, s)| (variant, s.strip_suffix(')')))
    {
        match variant {
            // TODO: make more extensible somehow
            "queue" => return Some(Cmd::Queue(arg.into())),
            "seek" => return arg.parse().ok().map(Cmd::Seek),
            "seek_relative" => return arg.parse().ok().map(Cmd::SeekRelative),
            "subscribe" => return arg.parse().ok().map(Cmd::Subscribe),
            "update" => return UpdateArgs::parse(arg).map(Cmd::Update),
            _ => {}
        }
    }
    None
}
