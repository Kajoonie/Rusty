# Rusty Discord Bot

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Discord](https://img.shields.io/badge/Discord-%235865F2.svg?style=for-the-badge&logo=discord&logoColor=white)](https://discord.com/)

A Discord bot built with Rust using the Poise framework, designed to enhance your server interactions with a variety of features.

## Table of Contents
- [Features](#features)
- [System Requirements](#system-requirements)
- [Setup](#setup)
- [Building and Running](#building-and-running)
- [Commands](#commands)
  - [General Commands](#general-commands)
  - [AI Commands](#ai-commands)
  - [Music Commands](#music-commands)
- [Development](#development)
- [Troubleshooting](#troubleshooting)
- [Contributing](#contributing)
- [License](#license)

## Features

- **General Commands**:
  - **Ping**: Responds with "Pong!" to check if the bot is online.
  - **Cryptocurrency Information**: Provides real-time updates on various cryptocurrencies.

- **AI Commands**:
  - **Chat**: Engages in conversation with users through an AI model that maintains context.
  - **Search**: Searches the web and provides AI-summarized results for any query.
  - **List Models**: Displays all available AI models that can be used.
  - **Set Model**: Changes which AI model is used for your interactions.
  - **Get Model**: Shows which AI model you're currently using.

- **Music Commands**:
  - **Play**: Initiates playback of a song from a URL or search query.
  - **Queue**: Displays the current music queue.
  - **Skip**: Skips to the next song in the queue.
  - **Stop**: Stops the current music playback and clears the queue.
  - **Leave**: Disconnects the bot from the voice channel.

## System Requirements

- Rust (latest stable version)
- Discord Bot Token
- Brave Search API Key
- YouTube Data API Key
- FFmpeg
- yt-dlp

## Setup

1. Clone the repository:
   ```bash
   git clone https://github.com/Kajoonie/Rusty.git
   cd Rusty
   ```

2. Install system dependencies:

   #### Windows
   ```bash
   winget install yt-dlp.yt-dlp
   winget install Gyan.FFmpeg
   ```

   #### macOS
   ```bash
   brew install yt-dlp ffmpeg
   ```

   #### Linux (Debian/Ubuntu)
   ```bash
   sudo curl -L https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp -o /usr/local/bin/yt-dlp
   sudo chmod a+rx /usr/local/bin/yt-dlp
   sudo apt update && sudo apt install ffmpeg
   ```

3. Create a `.env` file in the root directory:
   ```env
   # Required for basic bot functionality
   DISCORD_TOKEN=your_discord_bot_token
   
   # Required unless the `brave_search` feature is disabled
   BRAVE_API_KEY=your_brave_search_api_key
   
   # Required for the 'autoplay' function of the `music` feature
   SERP_API_KEY=your-serp-api-key
   ```

## Feature Flags

The bot supports feature flags to enable/disable specific functionality:

- `brave_search`: Enables the `/search` command that uses the Brave Search API
- `music`: Enables all music commands that use YouTube functionality

By default, all features are enabled. To build with only specific features:

```bash
# Build with only search functionality
cargo build --no-default-features --features "brave_search"

# Build with only music functionality
cargo build --no-default-features --features "music" 

# Build with both features explicitly
cargo build --features "brave_search music"

# Build with no optional features
cargo build --no-default-features
```

## Building and Running

Development build:
```bash
cargo build
cargo run
```

Production build:
```bash
cargo build --release
cargo run --release
```

## Commands

### General Commands
- `/ping`: Check bot responsiveness
- `/crypto <symbol>`: Get cryptocurrency information

### AI Commands
- `/chat <message>`: Chat with an AI model, maintaining conversation context
- `/search <query>`: Search the web and receive an AI-summarized response
- `/list_models`: View all available AI models on the server
- `/set_model <model>`: Change the AI model used for your interactions
- `/get_model`: Check which AI model you're currently using

### Music Commands
- `/play <URL or search term>`: Play a song from YouTube, Spotify, or a direct URL
  ```bash
  /play https://www.youtube.com/watch?v=dQw4w9WgXcQ
  ```
- `/skip`: Skip the current song
- `/stop`: Stop playing and clear the queue
- `/pause`: Pause/resume the current song
- `/queue`: View the current music queue
- `/leave`: Disconnect from the voice channel
- `/remove <position>`: Remove a song from the queue
- `/autoplay [enabled]`: Toggle or set autoplay feature

## Troubleshooting

Common issues and solutions:

1. **Bot doesn't respond**: Verify your Discord token and bot permissions
2. **Music doesn't play**: 
   - Check if FFmpeg is properly installed
   - Verify yt-dlp is up to date
   - Ensure bot has voice channel permissions
3. **API errors**: Verify your API keys in the `.env` file

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit changes (`git commit -m 'Add amazing feature'`)
4. Push to branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## New Features

### Autoplay
When enabled, the bot will automatically play related songs when the queue is empty. This feature uses YouTube's recommendations to find related songs.

### Spotify Integration
The bot can now play songs from Spotify URLs, including:
- Single tracks
- Playlists (queues all tracks)
- Albums (queues all tracks)

## Setup Instructions

### Basic Setup
1. Ensure your bot has the necessary permissions:
   - View Channels
   - Send Messages
   - Connect to Voice Channel
   - Speak in Voice Channel

### Spotify Integration Setup
To use Spotify integration, you need to set up a Spotify Developer application:

1. Go to [Spotify Developer Dashboard](https://developer.spotify.com/dashboard)
2. Create a new application
3. Get your Client ID and Client Secret
4. Set these as environment variables:
   ```
   SPOTIFY_CLIENT_ID=your_client_id
   SPOTIFY_CLIENT_SECRET=your_client_secret
   ```

## Requirements
- [yt-dlp](https://github.com/yt-dlp/yt-dlp) must be installed and available in PATH
- FFmpeg must be installed and available in PATH

## Development Notes
- The autoplay feature works by fetching related songs from YouTube when the queue is empty
- Spotify integration uses the Spotify API to fetch track information, then searches for the track on YouTube
- For playlists and albums, the first track is returned immediately, and the rest are processed and queued in the background