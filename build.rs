use std::path::Path;

fn main() {
    let dist = Path::new("frontend/dist");
    if !dist.exists() {
        let _ = std::fs::create_dir_all(dist);
    }
    println!("cargo:rerun-if-changed=frontend/dist");
    println!("cargo:rerun-if-changed=migrations");
}
