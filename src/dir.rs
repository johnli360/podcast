use std::{fs, path::PathBuf};

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
