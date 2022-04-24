use strum_macros::EnumString;
use strum_macros::{AsStaticStr, Display};

#[derive(Debug, EnumString, AsStaticStr, Display, PartialEq, Eq)]
#[strum(serialize_all = "snake_case")]
pub enum Cmd {
    Play,
    Pause,
    PlayPause,
    Queue(String),
    Shutdown,
    Seek(u64),
    SeekRelative(i64),
    Quit,
}

pub fn parse_cmd(buf: &str) -> Option<Cmd> {
    if let cmd @ Ok(_) = buf.trim_end().parse() {
        cmd.ok()
    } else if let cmd @ Some(_) = parse_cmd_arg(buf) {
        cmd
    } else {
        eprintln_raw!("failed to parse: {buf}");
        None
    }
}

fn parse_cmd_arg(buf: &str) -> Option<Cmd> {
    if let Some((variant, Some(arg))) = buf
        .split_once('(')
        .map(|(variant, s)| (variant, s.strip_suffix(')')))
    {
        println_raw!("variant: {variant}");
        match variant {
            // TODO: make more extensible somehow
            "queue" => return Some(Cmd::Queue(arg.into())),
            "seek" => return arg.parse().ok().map(Cmd::Seek),
            "seek_relative" => {
                return arg
                    .parse()
                    .ok()
                    .map(Cmd::SeekRelative)
            }
            _ => {}
        }
    }
    None
}


