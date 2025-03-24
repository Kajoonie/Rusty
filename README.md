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
  - [Music Commands](#music-commands)
- [Development](#development)
- [Troubleshooting](#troubleshooting)
- [Contributing](#contributing)
- [License](#license)

## Features

- **General Commands**:
  - **Ping**: Responds with "Pong!" to check if the bot is online.
  - **AI Responses**: Engages in conversation with users through text-based AI.
  - **Cryptocurrency Information**: Provides real-time updates on various cryptocurrencies.

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
   DISCORD_TOKEN=your_discord_bot_token
   BRAVE_API_KEY=your_brave_search_api_key
   YOUTUBE_API_KEY=your_youtube_data_api_key
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
- `/ai <message>`: Interact with AI
- `/crypto <symbol>`: Get cryptocurrency information

### Music Commands
- `/play <url or search query>`: Play audio from YouTube
  ```bash
  /play https://www.youtube.com/watch?v=dQw4w9WgXcQ
  ```
- `/queue`: Display current queue
- `/skip`: Skip current track
- `/stop`: Stop playback and clear queue
- `/leave`: Disconnect from voice channel

## Development

The music system is currently under development. Planned implementations:

1. YouTube-dl integration via Songbird
2. Queue management system
3. Audio playback controls
4. Voice channel management

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