# Rusty Discord Bot

A Discord bot built with Rust using the Poise framework, designed to enhance your server interactions with a variety of features.

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

## Setup

1. Clone the repository:
   ```bash
   git clone https://github.com/Kajoonie/Rusty.git
   ```

2. Create a `.env` file in the root directory with the following variables:
   ```
   DISCORD_TOKEN=your_discord_bot_token
   BRAVE_API_KEY=your_brave_search_api_key
   YOUTUBE_API_KEY=your_youtube_data_api_key
   ```

## Building and Running

To build and run the bot, execute the following commands:

```bash
cargo build --release
cargo run --release
```

## Music Commands (Detailed)

- **/play <url or search query>**: Acknowledges the command but doesn't play music yet. Replace `<url or search query>` with the actual URL of a song or a search term.
  
  Example:
  ```
  /play https://www.youtube.com/watch?v=dQw4w9WgXcQ
  ```

- **/queue**: Shows the current music queue.

- **/skip**: Skips to the next song in the queue.

- **/stop**: Stops the current music playback and clears the queue.

- **/leave**: Disconnects the bot from the voice channel.

## Future Music Implementation

To fully implement music functionality, the following steps are needed:

1. Properly configure Songbird to work with YouTube-dl or another audio source.
2. Implement actual audio playback in the play command.
3. Implement queue management.
4. Implement track skipping and stopping.
5. Implement voice channel leaving.

### Required Dependencies

For full music functionality, these system dependencies will be needed:

- [yt-dlp](https://github.com/yt-dlp/yt-dlp#installation) (for YouTube playback)
- FFmpeg (for audio processing)

#### Windows
```bash
# Install yt-dlp
winget install yt-dlp.yt-dlp

# Install FFmpeg
winget install Gyan.FFmpeg
```

#### macOS
```bash
# Install yt-dlp and FFmpeg using Homebrew
brew install yt-dlp ffmpeg
```

#### Linux (Debian/Ubuntu)
```bash
# Install yt-dlp
sudo curl -L https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp -o /usr/local/bin/yt-dlp
sudo chmod a+rx /usr/local/bin/yt-dlp

# Install FFmpeg
sudo apt update
sudo apt install ffmpeg
