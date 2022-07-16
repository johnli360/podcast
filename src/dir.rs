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

#[test]
fn lalala() {
    let c = children(PathBuf::from("/home/jl/downloads"));
    println!("{c:?}");
}

pub fn children(path: PathBuf) -> Vec<String> {
    if path.is_file() {
        return Vec::from([path.to_string_lossy().to_string()]);
    }

    let name: Option<String> = path.file_name().map(|n| n.to_string_lossy().to_string());
    let pred = |p: &PathBuf| {
        if let Some(name) = &name {
            p.to_string_lossy().to_string().contains(name)
        } else {
            true
        }
    };

    let dir_path = if path.is_dir() {
        path.as_path()
    } else if let Some(parent) = path.parent() {
        parent
    } else {
        return Vec::new();
    };

    if let Ok(it) = fs::read_dir(dir_path) {
        it.filter_map(Result::ok)
            .map(|e| e.path())
            .filter(pred)
            .map(|p| p.to_string_lossy().to_string())
            .collect()
    } else {
        Vec::new()
    }
}

pub fn get_file() -> Option<String> {
    disable_raw_mode().ok()?;
    println!("file: ");
    let mut s = String::from("file://");
    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    stdin.read_line(&mut s).ok()?;
    while s.ends_with('\n') {
        s.pop();
    }
    println!("line: {s}");
    enable_raw_mode().ok()?;
    Some(s)
}
