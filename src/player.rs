use std::process::Command;

pub fn play(input : Vec<String>) {

    const PLAYER: &str = "mpv";
    let mut cmd = Command::new(PLAYER);

    let cmd =
        input.iter().fold(&mut cmd, |c, l| (*c).arg(l));

    cmd.spawn().expect("mpv failed :(");
}
