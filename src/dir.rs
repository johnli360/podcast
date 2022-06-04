use std::{path::PathBuf, fs};
use std::io::{ self, BufRead};

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};


pub async fn files(start: &str) -> std::io::Result<Vec<PathBuf>> {
    let name = PathBuf::from(start);
    let handle = tokio::spawn(async {
        let mut stack = Vec::from([name]);
        let mut files = Vec::new();
        while let Some(mut it) = stack.pop().and_then(|p| fs::read_dir(p).ok()) {
            while let Some(Ok(entry)) = it.next() {
                if let Ok(ft) = entry.file_type() {
                    if ft.is_dir() {
                        stack.push(entry.path());
                    } else if ft.is_file() {
                        files.push(entry.path());
                        // if let Some(bd) = mk_build_data(&entry) {
                            // files.push(bd);
                        // }
                    }
                }
            }
        }
        files
    })
    .await?;
    Ok(Vec::new())
}

pub fn get_file() -> Option<String> {
    // let out = std::process::Command::new("fzf")
        // .stdout(stdio::)
        // .output().ok()?;
    // let out = ;
    disable_raw_mode().ok()?;
    // let s = String::from_utf8(out.stdout.to_vec()).ok();
    // println!();
    println!("file: ");
    let mut s = String::from("file://");
    // io::stdin().read_line(&mut s).ok()?;
    // print!("\rfile: ");
    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    stdin.read_line(&mut s).ok()?;
    while s.ends_with("\n") {
        s.pop();
    }
    // s.strip_suffix("\r\n").or(s.strip_suffix("\n"));
    // s.strip_suffix("\n")?;
    // s.strip
    // stdin.clear();
    // stdin.consume();
    println!("line: {s}");
    enable_raw_mode().ok()?;
    Some(s)
}

