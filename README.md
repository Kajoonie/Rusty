# Rusty Discord Bot

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Discord](https://img.shields.io/badge/Discord-%235865F2.svg?style=for-the-badge&logo=discord&logoColor=white)](https://discord.com/)
[![Version](https://img.shields.io/badge/version-1.1.34-blue.svg)](Cargo.toml)

A versatile Discord bot built with Rust using the Poise framework. Rusty enhances server interactions with AI capabilities, music playback, cryptocurrency information, and more.

## Table of Contents
- [Features](#features)
- [System Requirements](#system-requirements)
- [Setup](#setup)
- [Configuration](#configuration)
- [Feature Flags](#feature-flags)
- [Building](#building)
- [Usage](#usage)
- [Deployment](#deployment)
- [Commands](#commands)
- [Contributing](#contributing)
- [License](#license)

## Features

Rusty offers a range of functionalities powered by different command modules:

- **AI Integration (`ai` module):**
    - Engage in contextual conversations using Ollama models (`/chat`).
    - Get AI-summarized web search results via Brave Search (`/search`).
    - Manage available AI models (`/list_models`, `/set_model`, `/get_model`).
- **Music Playback (`music` module):**
    - Play audio from YouTube and Spotify (tracks, playlists, albums) (`/play`).
    - Manage the playback queue (`/remove`).
    - Toggle autoplay for related songs based on YouTube recommendations (`/autoplay`).
    - Control playback with embedded button controls for easier management.
- **Cryptocurrency Info (`coingecko` module):**
    - Fetch real-time cryptocurrency data from CoinGecko (`/coin price`).
- **General Utilities (`general` module):**
    - Check bot responsiveness (`/ping`).

## System Requirements

- **Rust:** Latest stable version recommended.
- **FFmpeg:** Required for audio processing (music feature).
- **yt-dlp:** Required for downloading audio from various sources (music feature).

## Setup

1.  **Clone the repository:**
    ```bash
    git clone https://github.com/Kajoonie/Rusty.git
    cd Rusty
    ```

2.  **Install System Dependencies:**
    *   **FFmpeg & yt-dlp:** These are required for the music features.

    *   **Windows (using winget):**
        ```bash
        winget install yt-dlp.yt-dlp
        winget install Gyan.FFmpeg
        ```
    *   **macOS (using Homebrew):**
        ```bash
        brew install yt-dlp ffmpeg
        ```
    *   **Linux (Debian/Ubuntu):**
        ```bash
        sudo curl -L https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp -o /usr/local/bin/yt-dlp
        sudo chmod a+rx /usr/local/bin/yt-dlp
        sudo apt update && sudo apt install ffmpeg -y
        ```

3.  **Create Configuration File:**
    *   Copy the example environment file:
        ```bash
        cp .env.example .env
        ```
    *   Edit the `.env` file with your credentials (see [Configuration](#configuration) below).

## Configuration

Configure the bot by editing the `.env` file in the project root. Refer to `.env.example` for a template.

**Required Variables:**

-   `DISCORD_TOKEN`: Your Discord bot token.

**Optional Variables (Required for specific features):**

-   `BRAVE_API_KEY`: Required for the `/search` command (if `brave_search` feature is enabled).
-   `SERP_API_KEY`: Required for the `/autoplay` functionality (if `music` feature is enabled).
-   `SPOTIFY_CLIENT_ID` & `SPOTIFY_CLIENT_SECRET`: Required for Spotify integration (if `music` feature is enabled).
    *   To get these, create an application on the [Spotify Developer Dashboard](https://developer.spotify.com/dashboard).

## Feature Flags

Control optional features using Cargo feature flags:

-   `brave_search`: Enables the `/search` command (uses Brave Search API).
-   `music`: Enables all music commands (uses yt-dlp, FFmpeg, Spotify API, SERP API).

**Default:** Both `brave_search` and `music` are enabled by default.

**Build/Run Examples:**

```bash
# Build/Run with default features (all enabled)
cargo build
cargo run

# Build/Run with only search functionality
cargo build --no-default-features --features "brave_search"
cargo run --no-default-features --features "brave_search"

# Build/Run with only music functionality
cargo build --no-default-features --features "music"
cargo run --no-default-features --features "music"

# Build/Run with no optional features
cargo build --no-default-features
cargo run --no-default-features
```

## Building

Compile the project using Cargo:

-   **Development Build:**
    ```bash
    cargo build
    ```
-   **Production Build (Optimized):**
    ```bash
    cargo build --release
    ```

## Usage

Run the compiled bot:

-   **Development:**
    ```bash
    cargo run
    ```
-   **Production:**
    ```bash
    cargo run --release
    ```

## Deployment

The production bot runs under systemd (`rusty.service`) in a small Debian 12 container. It is not built there: every push to `main` is built by GitHub Actions and the container picks up the finished binary by itself.

**How it works**

1. `.github/workflows/release.yml` runs the tests and `cargo build --release --locked` inside a `debian:12` container, then replaces the assets of the rolling [`latest`](https://github.com/Kajoonie/Rusty/releases/tag/latest) release: `rusty` (the binary), `rusty.sha256` and `BUILD_INFO` (commit, commit date, link to the run).
2. On the container, `rusty-autodeploy.timer` runs `rusty-autodeploy.sh` every 5 minutes. It downloads `rusty.sha256` and exits silently when that matches the deployed binary.
3. Otherwise it downloads the binary, checks the checksum, that it is an x86_64 program and that every library it needs is installed, keeps the current binary as `target/release/rusty.prev`, moves the new one to `/root/Rusty/target/release/rusty` and restarts `rusty.service`. A restart stops whatever the bot is playing.
4. If the bot is not running 20 seconds after the restart, the script puts `rusty.prev` back, restarts again and fails. That build is skipped from then on; the next build that is published is tried again.
5. After a successful deploy the `/root/Rusty` checkout is fast-forwarded to the deployed commit, but only when it is on `main` and has no local changes. `.env` and `application_data.db` are never touched.

If GitHub cannot be reached or a release is half uploaded, nothing changes and the next run tries again. If `rusty.service` was stopped by hand, the new binary is installed but the bot is not started.

**One-time install**

Run as root inside the container, after the first run of the `Release` workflow has finished:

```bash
cd /root/Rusty && git fetch origin main && \
git show origin/main:deploy/rusty-autodeploy.sh      > /usr/local/sbin/rusty-autodeploy.sh && \
git show origin/main:deploy/rusty-autodeploy.service > /etc/systemd/system/rusty-autodeploy.service && \
git show origin/main:deploy/rusty-autodeploy.timer   > /etc/systemd/system/rusty-autodeploy.timer && \
chmod 0755 /usr/local/sbin/rusty-autodeploy.sh && \
systemctl daemon-reload && echo "rusty-autodeploy installed"
```

```bash
systemctl start rusty-autodeploy.service            # first deploy: restarts the bot
journalctl -u rusty-autodeploy -n 20 --no-pager     # expect a "deployed build ..." line
systemctl enable --now rusty-autodeploy.timer
```

The binary that is there today was built by hand, so its checksum differs from the release and the first run replaces it with the release build. That is intended; the hand-built binary stays available as `rusty.prev`.

If the first start fails with `status=226/NAMESPACE`, the container does not allow systemd's file system sandbox (an LXC container without nesting). The bot is untouched in that case. Turn that part of the hardening off and start again:

```bash
mkdir -p /etc/systemd/system/rusty-autodeploy.service.d
printf '[Service]\nProtectSystem=no\nProtectHome=no\nPrivateTmp=no\nReadWritePaths=\n' \
  > /etc/systemd/system/rusty-autodeploy.service.d/no-fs-sandbox.conf
systemctl daemon-reload
```

The installed copies are what runs as root. Later changes to `deploy/` in the repository do nothing until the install commands are run again. Settings (paths, unit name, wait time) are at the top of the script and can be overridden in `/etc/default/rusty-autodeploy`.

**Status and logs**

```bash
systemctl list-timers rusty-autodeploy.timer    # last and next check
journalctl -u rusty-autodeploy                  # one line per action, nothing while up to date
cat /var/lib/rusty-autodeploy/BUILD_INFO        # the build that is deployed
```

**Pause**

```bash
systemctl disable --now rusty-autodeploy.timer  # resume: systemctl enable --now rusty-autodeploy.timer
```

Pause it before running a binary you built yourself with `cargo build --release`: its checksum differs from the release, so the next check would replace it.

**Roll back**

```bash
systemctl disable --now rusty-autodeploy.timer  # otherwise the release is deployed again within 5 minutes
cd /root/Rusty/target/release && cp -p rusty.prev rusty.tmp && mv rusty.tmp rusty
systemctl restart rusty.service
```

Enable the timer again once a fixed commit has been pushed to `main` and its `Release` run has finished. A build that was rolled back automatically is recorded in `/var/lib/rusty-autodeploy/hold`; delete that file to have it tried again.

## Commands

Here are the primary commands available, grouped by module:

**General:**
-   `/ping`: Checks if the bot is responsive.

**AI:**
-   `/chat <message>`: Start or continue a conversation with the configured Ollama AI model.
-   `/search <query>`: Perform a web search using Brave Search and get an AI-summarized answer.
-   `/list_models`: Show available Ollama models.
-   `/set_model <model_name>`: Set the Ollama model for your interactions.
-   `/get_model`: Display the currently set Ollama model.

**CoinGecko:**
-   `/coin price <name>`: Get price information for a specific cryptocurrency.

**Music:**
-   `/play <url_or_search_query>`: Play audio from YouTube/Spotify URL or search term. Queues playlists/albums.
-   `/autoplay [true/false]`: Enable or disable automatic playback of related songs when the queue is empty.
-   `/remove <position>`: Remove a song from the queue by its position number.

## Contributing

Contributions are welcome! Please follow these steps:

1.  Fork the repository.
2.  Create a feature branch (`git checkout -b feature/your-feature`).
3.  Commit your changes (`git commit -m 'Add some feature'`).
4.  Push to the branch (`git push origin feature/your-feature`).
5.  Open a Pull Request.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.