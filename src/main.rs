mod app;
mod catalog;
mod distro;
mod installer;
mod resolver;

fn main() -> Result<(), String> {
    let mut app = app::App::new().map_err(|e| e.to_string())?;
    if let Err(e) = app.run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
    Ok(())
}