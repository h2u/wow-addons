// Standalone Windows fixture: acknowledge startup, then exit without touching app data.
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    assert_eq!(args.len(), 3);
    assert_eq!(args[1], "--portable-update-ready");
    std::fs::write(std::path::Path::new(&args[2]).join("started"), b"").unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1));
}
