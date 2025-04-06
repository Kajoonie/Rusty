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