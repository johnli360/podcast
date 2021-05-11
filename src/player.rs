use std::process::{Command, Stdio};

pub fn play(input: Vec<String>, block: bool) {
    const PLAYER: &str = "mpv";
    let mut cmd = Command::new(PLAYER);

    let cmd = input.iter().fold(&mut cmd, |c, l| (*c).arg(l));

    if block {
        cmd.stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .expect("mpv failed :/");
    } else {
        cmd.spawn().expect("mpv failed :(");
    }
}
