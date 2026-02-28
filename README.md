# 🦀 Rusty Rebase

**Rusty Rebase** is a fast, declarative, and interactive Linux post-installation setup and software management tool built in Rust. 

Tired of running messy bash scripts to set up a new Linux machine? Rusty Rebase allows you to define your desired software environment in a clean TOML configuration file and handles the heavy lifting of resolving download URLs, parsing GitHub releases, and utilizing your system's package manager. Complete with a beautiful Terminal User Interface (TUI).

## Features

- **Declarative Configuration:** Define everything you want to install in a simple `software_catalog.toml` file.
- **Interactive TUI:** Built with `ratatui`, offering a clean and intuitive interface for selecting, resolving, and installing applications.
- **Multiple Source Types:**
  - `package_manager`: Install from your distro's native repositories.
  - `official_source`: Direct downloads with dynamic version/URL resolution using Regular Expressions.
  - `github`: Automatically fetch the latest release assets from GitHub repositories.
- **Cross-Distribution:** Automatically detects your Linux distribution and uses the appropriate package manager (APT, DNF, or Pacman).
- **Dry Run Mode:** Preview exactly what commands will be executed without modifying your system.
- **Automated Setup:** Supports pre/post-installation steps including custom shell commands, package dependencies, and automatically injecting variables into your `PATH` profile (`.bashrc`, `.zshrc`, `.config/fish/config.fish`).
- **Concurrent Execution:** Fast resolution of download URLs and multi-threaded logging.

## Installation

### Prerequisites

You need Rust installed on your machine to build and run Rusty Rebase.

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Build from Source

```bash
git clone https://github.com/iamthetwodigiter/rusty_rebase.git
cd rusty_rebase
cargo build --release
```

## Usage

Run Rusty Rebase using Cargo or the compiled binary:

```bash
cargo run
# OR
./target/release/rusty_rebase
```

### TUI Keybindings

- <kbd>↑</kbd> / <kbd>↓</kbd>: Navigate the software catalog
- <kbd>Space</kbd>: Select or deselect a package for installation
- <kbd>a</kbd>: Select all packages
- <kbd>n</kbd>: Deselect all packages
- <kbd>r</kbd>: Resolve URLs and versions for selected packages
- <kbd>i</kbd>: Start the installation process
- <kbd>d</kbd>: Toggle **Dry Run** mode (highly recommended for previewing actions)
- <kbd>c</kbd>: Cancel current installation (Ctrl+c also supported)
- <kbd>q</kbd>: Quit the application

## Configuration (`software_catalog.toml`)

The power of Rusty Rebase lies in its catalog file. You can easily add new software, specify custom install directories, and define complex setup steps.

Here is an example of adding a new package:

```toml
[software.golang]
display_name = "Go Lang"
description = "Open source programming language"
category = "Development"
enabled_by_default = false
install_dir = "~/"

# Source configuration
[software.golang.source]
kind = "official_source"
url = "https://go.dev/dl/"
version_regex = "go([0-9\\.]+)\\.linux"
download_url_regex = "/dl/go[0-9\\.]+\\.linux-{arch}\\.tar\\.gz"

# Pre/Post setup steps
[[software.golang.setup_steps]]
kind = "package"
packages = ["git", "build-essential"]

[[software.golang.setup_steps]]
kind = "path_hint"
value = "<install_root>/go/bin"
```

### Available Setup Steps
- `package`: Installs dependent libraries via your package manager.
- `path_hint`: Appends the path to your shell's profile.
- `shell`: Executes arbitrary shell commands. Supports architecture variables like `{arch}` and `{xarch}`.
- `note`: Displays helpful instructions to the user.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
1. Fork the Project
2. Create your Feature Branch (`git checkout -b feature/AmazingFeature`)
3. Commit your Changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the Branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

---

Developed with ❤️ by [thetwodigiter](https://www.thetwodigiter.app)