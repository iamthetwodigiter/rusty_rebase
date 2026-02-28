mod app;
mod catalog;
mod distro;
mod installer;
mod resolver;
mod restorer;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "restore" {
        let backup_dir = std::path::Path::new(&args[2]);
        match restorer::restore_backup(backup_dir) {
            Ok(logs) => {
                for log in logs {
                    println!("{}", log);
                }
                return Ok(());
            }
            Err(e) => return Err(e),
        }
    }

    let mut app = app::App::new().map_err(|e| e.to_string())?;
    if let Err(e) = app.run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
    Ok(())
}