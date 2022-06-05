use std::io::{self, BufRead};
use std::{fs, path::PathBuf};

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

pub async fn files(start: &str) -> std::io::Result<Vec<PathBuf>> {
    let name = PathBuf::from(start);
    let _handle = tokio::spawn(async {
        let mut stack = Vec::from([name]);
        let mut files = Vec::new();
        while let Some(mut it) = stack.pop().and_then(|p| fs::read_dir(p).ok()) {
            while let Some(Ok(entry)) = it.next() {
                if let Ok(ft) = entry.file_type() {
                    if ft.is_dir() {
                        stack.push(entry.path());
                    } else if ft.is_file() {
                        files.push(entry.path());
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
    disable_raw_mode().ok()?;
    println!("file: ");
    let mut s = String::from("file://");
    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    stdin.read_line(&mut s).ok()?;
    while s.ends_with("\n") {
        s.pop();
    }
    println!("line: {s}");
    enable_raw_mode().ok()?;
    Some(s)
}
