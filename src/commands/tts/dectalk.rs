//! Runs the external DECtalk `say` program and turns its output into a playable clip.
//!
//! DECtalk is old C code that parses whatever a Discord user typed, so the process is
//! started with as little as possible: no shell, the text on stdin instead of the
//! command line, an almost empty environment, a time limit, and (when the bot runs as
//! root) an unprivileged user.

use std::env;
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::LazyLock;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tracing::warn;

use super::{TtsError, TtsResult};

/// Environment variable holding the path to the DECtalk `say` executable.
pub const DECTALK_PATH_ENV: &str = "DECTALK_PATH";
/// Where `say` is expected when `DECTALK_PATH` is not set.
pub const DEFAULT_DECTALK_PATH: &str = "/opt/dectalk/say";
/// Environment variable naming the user DECtalk runs as when the bot is root.
pub const DECTALK_RUN_AS_ENV: &str = "DECTALK_RUN_AS";
/// The user DECtalk runs as when the bot is root and `DECTALK_RUN_AS` is not set.
pub const DEFAULT_RUN_AS_USER: &str = "nobody";
/// Uid and gid used when the configured user is not listed in `/etc/passwd`.
#[cfg(unix)]
const FALLBACK_UNPRIVILEGED_ID: u32 = 65_534;

/// Longest a single synthesis may take before the process is killed.
pub const SYNTH_TIMEOUT: Duration = Duration::from_secs(5);
/// Longest clip that is played; longer audio is cut off.
pub const MAX_AUDIO_SECONDS: u32 = 180;
/// Sample rate of DECtalk's output (16-bit mono PCM).
pub const SAMPLE_RATE: u32 = 11_025;
/// Bytes per 16-bit mono sample.
const BYTES_PER_SAMPLE: usize = 2;
/// Size of `MAX_AUDIO_SECONDS` of audio in bytes.
pub const MAX_PCM_BYTES: usize =
    MAX_AUDIO_SECONDS as usize * SAMPLE_RATE as usize * BYTES_PER_SAMPLE;

/// Turns on phoneme interpretation, so strings like `[dah<600,20>]` sing as in Moonbase Alpha.
const PHONEME_PREFIX: &str = "[:phoneme on]";
/// `PATH` given to the DECtalk process. The bot's own environment is not passed on.
const CHILD_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
/// How much of DECtalk's error output is kept for the log.
const STDERR_LOG_LIMIT: usize = 2_048;

/// The user and group the DECtalk process is switched to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(unix), allow(dead_code))]
pub struct RunAs {
    /// Numeric user id.
    pub uid: u32,
    /// Numeric group id.
    pub gid: u32,
}

/// Where DECtalk is installed and how it is started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DectalkConfig {
    /// Absolute path to the `say` executable.
    pub say_path: PathBuf,
    /// The user to switch to before running `say`; `None` keeps the bot's own user.
    pub run_as: Option<RunAs>,
}

/// Global DECtalk configuration, read from the environment on first use.
static CONFIG: LazyLock<DectalkConfig> = LazyLock::new(DectalkConfig::from_env);

/// Returns the DECtalk configuration read from `DECTALK_PATH` and `DECTALK_RUN_AS`.
pub fn config() -> &'static DectalkConfig {
    &CONFIG
}

impl DectalkConfig {
    /// Reads the configuration from the environment.
    pub fn from_env() -> Self {
        Self {
            say_path: resolve_say_path(env::var_os(DECTALK_PATH_ENV)),
            run_as: run_as_from_env(),
        }
    }

    /// Whether the `say` executable exists.
    pub fn is_installed(&self) -> bool {
        self.say_path.is_file()
    }

    /// The directory containing `say`. DECtalk runs with this as its working directory.
    pub fn install_dir(&self) -> PathBuf {
        match self.say_path.parent() {
            Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
            _ => PathBuf::from("/"),
        }
    }

    /// The `lib` directory beside `say`, holding DECtalk's shared libraries.
    pub fn lib_dir(&self) -> PathBuf {
        self.install_dir().join("lib")
    }
}

/// Picks the configured path, or the default when it is unset or empty, as an absolute path.
fn resolve_say_path(configured: Option<OsString>) -> PathBuf {
    let path = configured
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_DECTALK_PATH));

    // DECtalk runs with a different working directory, so a relative path must be pinned down.
    std::path::absolute(&path).unwrap_or(path)
}

/// Decides which user DECtalk runs as, from the bot's own user and `DECTALK_RUN_AS`.
#[cfg(unix)]
fn run_as_from_env() -> Option<RunAs> {
    // SAFETY: `geteuid` only reads the process credentials and cannot fail.
    let euid = unsafe { libc::geteuid() };

    let user = env::var(DECTALK_RUN_AS_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_RUN_AS_USER.to_string());

    let passwd = std::fs::read_to_string("/etc/passwd").unwrap_or_default();

    resolve_run_as(euid, &user, &passwd)
}

/// Dropping privileges is only supported on Unix.
#[cfg(not(unix))]
fn run_as_from_env() -> Option<RunAs> {
    None
}

/// Decides whether to drop privileges for DECtalk, and to which user.
///
/// * The bot is not root: `None`, since it cannot switch users.
/// * The bot is root: the `user` entry of `passwd` (the contents of `/etc/passwd`), or
///   uid/gid 65534 if there is no such entry.
/// * The bot is root and `user` is itself a root account: `None`. This is the explicit
///   way to opt out.
#[cfg(unix)]
fn resolve_run_as(euid: u32, user: &str, passwd: &str) -> Option<RunAs> {
    if euid != 0 {
        return None;
    }

    let target = lookup_user(passwd, user).unwrap_or_else(|| {
        warn!(
            "User '{}' not found in /etc/passwd; DECtalk will run as uid/gid {}",
            user, FALLBACK_UNPRIVILEGED_ID
        );
        RunAs {
            uid: FALLBACK_UNPRIVILEGED_ID,
            gid: FALLBACK_UNPRIVILEGED_ID,
        }
    });

    if target.uid == 0 {
        warn!(
            "{} names a root account; DECtalk will run with full privileges",
            DECTALK_RUN_AS_ENV
        );
        return None;
    }

    Some(target)
}

/// Finds a user's uid and gid in the contents of `/etc/passwd`.
#[cfg(unix)]
fn lookup_user(passwd: &str, user: &str) -> Option<RunAs> {
    passwd.lines().find_map(|line| {
        let mut fields = line.split(':');

        if fields.next()? != user {
            return None;
        }

        // Skip the password placeholder.
        fields.next()?;
        let uid = fields.next()?.parse().ok()?;
        let gid = fields.next()?.parse().ok()?;

        Some(RunAs { uid, gid })
    })
}

/// The arguments given to `say`.
///
/// The text is deliberately not one of them: `say -a "-h"` treats the text as an
/// option. It is written to stdin instead, which the final `-` tells `say` to read.
fn say_args() -> [&'static str; 5] {
    ["-pre", PHONEME_PREFIX, "-fo", "stdout:raw", "-"]
}

/// Builds the `LD_LIBRARY_PATH` for DECtalk: its own `lib` directory, then any existing value.
fn library_path(lib_dir: &Path, existing: Option<&OsStr>) -> OsString {
    let mut value = lib_dir.as_os_str().to_os_string();

    if let Some(existing) = existing.filter(|existing| !existing.is_empty()) {
        value.push(":");
        value.push(existing);
    }

    value
}

/// Restricts what a child process gets from the bot: an almost empty environment,
/// DECtalk's directory as working directory, and an unprivileged user if configured.
fn confine(command: &mut Command, config: &DectalkConfig) {
    command
        // Keep the Discord token and API keys out of the child's environment.
        .env_clear()
        .env(
            "LD_LIBRARY_PATH",
            library_path(&config.lib_dir(), env::var_os("LD_LIBRARY_PATH").as_deref()),
        )
        .env("PATH", CHILD_PATH)
        .current_dir(config.install_dir())
        .kill_on_drop(true);

    // Setting a uid as root also makes the standard library clear the supplementary groups.
    #[cfg(unix)]
    if let Some(run_as) = config.run_as {
        command.uid(run_as.uid).gid(run_as.gid);
    }
}

/// Builds the command that runs `say`, without starting it.
fn build_command(config: &DectalkConfig) -> Command {
    let mut command = Command::new(&config.say_path);

    command
        .args(say_args())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    confine(&mut command, config);

    command
}

/// Raw audio returned by `say`.
#[derive(Debug, PartialEq, Eq)]
struct SayOutput {
    /// 16-bit mono PCM samples in host byte order.
    pcm: Vec<u8>,
    /// Whether the audio was longer than allowed and has been cut.
    cut_off: bool,
}

/// What was collected from a `say` process.
struct Collected {
    /// Everything read from stdout, at most one byte more than the limit.
    pcm: Vec<u8>,
    /// Whether the process exited successfully. `None` if it was stopped at the limit.
    success: Option<bool>,
    /// The start of what the process wrote to stderr.
    stderr: String,
}

/// Reads up to `limit` bytes as text and discards the rest, so the writer never blocks.
async fn read_limited<R: AsyncRead + Unpin>(mut reader: R, limit: usize) -> String {
    let mut kept = Vec::new();
    let _ = (&mut reader)
        .take(limit as u64)
        .read_to_end(&mut kept)
        .await;
    let _ = tokio::io::copy(&mut reader, &mut tokio::io::sink()).await;

    String::from_utf8_lossy(&kept).into_owned()
}

/// Stops a `say` process and waits for it to be reaped.
async fn kill(child: &mut Child) {
    if let Err(e) = child.kill().await {
        warn!("Failed to stop DECtalk process: {}", e);
    }
}

/// Feeds `text` to a running `say` process and collects its output.
///
/// Reading stops one byte past `max_pcm_bytes`; the process is then killed, as
/// whatever else it would produce is discarded anyway.
async fn collect_output(
    child: &mut Child,
    text: &str,
    max_pcm_bytes: usize,
) -> io::Result<Collected> {
    let missing_pipe = || io::Error::other("DECtalk process has no pipe");
    let mut stdin = child.stdin.take().ok_or_else(missing_pipe)?;
    let stdout = child.stdout.take().ok_or_else(missing_pipe)?;
    let stderr = child.stderr.take().ok_or_else(missing_pipe)?;

    let stderr_task = tokio::spawn(read_limited(stderr, STDERR_LOG_LIMIT));

    let write_text = async move {
        // A process that exits early closes the pipe; its exit status reports the problem.
        let _ = stdin.write_all(text.as_bytes()).await;
        let _ = stdin.shutdown().await;
    };
    let read_audio = async move {
        let mut pcm = Vec::new();
        stdout
            .take(max_pcm_bytes as u64 + 1)
            .read_to_end(&mut pcm)
            .await
            .map(|_| pcm)
    };

    let ((), pcm) = tokio::join!(write_text, read_audio);
    let pcm = pcm?;

    if pcm.len() > max_pcm_bytes {
        kill(child).await;
        stderr_task.abort();

        return Ok(Collected {
            pcm,
            success: None,
            stderr: String::new(),
        });
    }

    let status = child.wait().await?;
    let stderr = stderr_task.await.unwrap_or_default();

    Ok(Collected {
        pcm,
        success: Some(status.success()),
        stderr,
    })
}

/// Cuts PCM audio down to `max_bytes` and to whole samples.
///
/// Returns whether audio was lost by the cut.
fn cap_pcm(pcm: &mut Vec<u8>, max_bytes: usize) -> bool {
    let cut_off = pcm.len() > max_bytes;

    if cut_off {
        pcm.truncate(max_bytes);
    }
    pcm.truncate(pcm.len() - pcm.len() % BYTES_PER_SAMPLE);

    cut_off
}

/// Runs `say` on `text` and returns its audio, limited in time and size.
async fn run_say(
    config: &DectalkConfig,
    text: &str,
    timeout: Duration,
    max_pcm_bytes: usize,
) -> TtsResult<SayOutput> {
    if !config.is_installed() {
        return Err(TtsError::NotInstalled);
    }

    let mut child = build_command(config).spawn().map_err(|e| {
        warn!(
            "Failed to start DECtalk at {}: {}",
            config.say_path.display(),
            e
        );
        match e.kind() {
            io::ErrorKind::NotFound => TtsError::NotInstalled,
            _ => TtsError::SpawnFailed,
        }
    })?;

    let collected = match tokio::time::timeout(
        timeout,
        collect_output(&mut child, text, max_pcm_bytes),
    )
    .await
    {
        Ok(Ok(collected)) => collected,
        Ok(Err(e)) => {
            kill(&mut child).await;
            warn!("Failed to read DECtalk output: {}", e);
            return Err(TtsError::SynthesisFailed);
        }
        Err(_) => {
            kill(&mut child).await;
            warn!("DECtalk did not finish within {:?} and was killed", timeout);
            return Err(TtsError::Timeout(timeout));
        }
    };

    if collected.success == Some(false) {
        warn!(
            "DECtalk exited with an error. stderr: {}",
            collected.stderr.trim()
        );
        return Err(TtsError::SynthesisFailed);
    }

    let mut pcm = collected.pcm;
    let cut_off = cap_pcm(&mut pcm, max_pcm_bytes);

    if pcm.is_empty() {
        return Err(TtsError::NoAudio);
    }

    Ok(SayOutput { pcm, cut_off })
}

/// Length of 16-bit mono PCM audio at DECtalk's sample rate.
fn pcm_duration(pcm_bytes: usize) -> Duration {
    let samples = (pcm_bytes / BYTES_PER_SAMPLE) as f64;
    Duration::from_secs_f64(samples / f64::from(SAMPLE_RATE))
}

/// Wraps 16-bit mono PCM samples (host byte order) in a WAV header.
pub fn wav_from_pcm(pcm: &[u8]) -> Vec<u8> {
    const CHANNELS: u16 = 1;
    const BITS_PER_SAMPLE: u16 = 16;
    const PCM_FORMAT: u16 = 1;
    const FMT_CHUNK_SIZE: u32 = 16;
    const HEADER_SIZE: usize = 44;

    // Whole samples only.
    let pcm = &pcm[..pcm.len() - pcm.len() % BYTES_PER_SAMPLE];

    let block_align = CHANNELS * BITS_PER_SAMPLE / 8;
    let byte_rate = SAMPLE_RATE * u32::from(block_align);
    let data_size = pcm.len() as u32;

    let mut wav = Vec::with_capacity(HEADER_SIZE + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(HEADER_SIZE as u32 - 8 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&FMT_CHUNK_SIZE.to_le_bytes());
    wav.extend_from_slice(&PCM_FORMAT.to_le_bytes());
    wav.extend_from_slice(&CHANNELS.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());

    // WAV samples are little-endian; DECtalk writes the host's byte order.
    let mut bytes = pcm.iter().copied();
    while let (Some(first), Some(second)) = (bytes.next(), bytes.next()) {
        let sample = i16::from_ne_bytes([first, second]);
        wav.extend_from_slice(&sample.to_le_bytes());
    }

    wav
}

/// Synthesized speech, ready to be handed to Songbird.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clip {
    /// The audio as a complete in-memory WAV file.
    pub wav: Vec<u8>,
    /// How long the audio plays.
    pub duration: Duration,
    /// Whether the audio hit `MAX_AUDIO_SECONDS` and was cut.
    pub cut_off: bool,
}

/// Speaks `text` with DECtalk and returns the audio as a clip.
///
/// `text` should already be cleaned with `text::validate_text`. The process is killed
/// after `SYNTH_TIMEOUT`, and the audio is cut at `MAX_AUDIO_SECONDS`.
pub async fn synthesize(config: &DectalkConfig, text: &str) -> TtsResult<Clip> {
    // DECtalk produces no audio at all for input with a non-ASCII byte (curly quotes
    // from a phone keyboard, accents, emoji), so reduce the text to ASCII first.
    let text = super::answer::to_ascii(text);
    let output = run_say(config, &text, SYNTH_TIMEOUT, MAX_PCM_BYTES).await?;

    Ok(Clip {
        duration: pcm_duration(output.pcm.len()),
        wav: wav_from_pcm(&output.pcm),
        cut_off: output.cut_off,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes the tests that start processes: writing an executable while another
    /// thread forks can make running it fail with "text file busy".
    #[cfg(unix)]
    static PROCESS_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    fn config_for(say_path: &str) -> DectalkConfig {
        DectalkConfig {
            say_path: PathBuf::from(say_path),
            run_as: None,
        }
    }

    #[test]
    fn test_say_args_enable_phonemes_and_read_stdin() {
        assert_eq!(
            say_args(),
            ["-pre", "[:phoneme on]", "-fo", "stdout:raw", "-"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_resolve_say_path_defaults() {
        assert_eq!(resolve_say_path(None), PathBuf::from("/opt/dectalk/say"));
        assert_eq!(
            resolve_say_path(Some(OsString::new())),
            PathBuf::from(DEFAULT_DECTALK_PATH)
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_resolve_say_path_uses_configured_path() {
        assert_eq!(
            resolve_say_path(Some(OsString::from("/srv/dectalk/say"))),
            PathBuf::from("/srv/dectalk/say")
        );
    }

    #[test]
    fn test_resolve_say_path_makes_relative_paths_absolute() {
        assert!(resolve_say_path(Some(OsString::from("dectalk/say"))).is_absolute());
    }

    #[test]
    fn test_install_and_lib_dir_are_beside_say() {
        let config = config_for("/opt/dectalk/say");
        assert_eq!(config.install_dir(), PathBuf::from("/opt/dectalk"));
        assert_eq!(config.lib_dir(), PathBuf::from("/opt/dectalk/lib"));
    }

    #[test]
    fn test_library_path_prepends_lib_dir() {
        let lib_dir = Path::new("/opt/dectalk/lib");
        assert_eq!(library_path(lib_dir, None), "/opt/dectalk/lib");
        assert_eq!(
            library_path(lib_dir, Some(OsStr::new(""))),
            "/opt/dectalk/lib"
        );
        assert_eq!(
            library_path(lib_dir, Some(OsStr::new("/usr/local/lib:/x"))),
            "/opt/dectalk/lib:/usr/local/lib:/x"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_build_command_arguments_and_environment() {
        let config = config_for("/opt/dectalk/say");
        let command = build_command(&config);
        let command = command.as_std();

        assert_eq!(command.get_program(), "/opt/dectalk/say");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["-pre", "[:phoneme on]", "-fo", "stdout:raw", "-"]
        );
        assert_eq!(command.get_current_dir(), Some(Path::new("/opt/dectalk")));

        // Only these two variables are set explicitly; everything else is cleared.
        let mut names: Vec<_> = command
            .get_envs()
            .filter(|(_, value)| value.is_some())
            .map(|(name, _)| name.to_os_string())
            .collect();
        names.sort();
        assert_eq!(names, ["LD_LIBRARY_PATH", "PATH"]);

        let lib_path = command
            .get_envs()
            .find(|(name, _)| *name == "LD_LIBRARY_PATH")
            .and_then(|(_, value)| value)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(lib_path == "/opt/dectalk/lib" || lib_path.starts_with("/opt/dectalk/lib:"));
    }

    #[test]
    fn test_cap_pcm_leaves_short_audio_alone() {
        let mut pcm = vec![1u8; 1_000];
        assert!(!cap_pcm(&mut pcm, MAX_PCM_BYTES));
        assert_eq!(pcm.len(), 1_000);
    }

    #[test]
    fn test_cap_pcm_cuts_audio_at_the_cap() {
        // One second more than the cap.
        let mut pcm = vec![0u8; MAX_PCM_BYTES + SAMPLE_RATE as usize * BYTES_PER_SAMPLE];
        assert!(cap_pcm(&mut pcm, MAX_PCM_BYTES));
        assert_eq!(pcm.len(), MAX_PCM_BYTES);
        assert_eq!(
            pcm_duration(pcm.len()),
            Duration::from_secs(u64::from(MAX_AUDIO_SECONDS))
        );
    }

    #[test]
    fn test_cap_pcm_at_exactly_the_limit_is_not_cut_off() {
        let mut pcm = vec![0u8; MAX_PCM_BYTES];
        assert!(!cap_pcm(&mut pcm, MAX_PCM_BYTES));
        assert_eq!(pcm.len(), MAX_PCM_BYTES);
    }

    #[test]
    fn test_cap_pcm_drops_half_a_sample() {
        let mut pcm = vec![0u8; 7];
        assert!(!cap_pcm(&mut pcm, MAX_PCM_BYTES));
        assert_eq!(pcm.len(), 6);

        let mut pcm = vec![0u8; 20];
        assert!(cap_pcm(&mut pcm, 5));
        assert_eq!(pcm.len(), 4);
    }

    #[test]
    fn test_max_pcm_bytes_matches_max_audio_seconds() {
        assert_eq!(MAX_PCM_BYTES, 3_969_000);
        assert_eq!(
            pcm_duration(MAX_PCM_BYTES),
            Duration::from_secs(u64::from(MAX_AUDIO_SECONDS))
        );
    }

    #[test]
    fn test_pcm_duration() {
        assert_eq!(pcm_duration(0), Duration::ZERO);
        assert_eq!(pcm_duration(22_050), Duration::from_secs(1));
        assert_eq!(pcm_duration(44_100), Duration::from_secs(2));
    }

    #[test]
    fn test_wav_from_pcm_header() {
        let samples: [i16; 4] = [0, 1, -2, i16::MAX];
        let pcm: Vec<u8> = samples.iter().flat_map(|s| s.to_ne_bytes()).collect();
        let wav = wav_from_pcm(&pcm);

        assert_eq!(wav.len(), 44 + 8);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(wav[4..8].try_into().unwrap()), 36 + 8);
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(u32::from_le_bytes(wav[16..20].try_into().unwrap()), 16);
        // PCM, mono.
        assert_eq!(u16::from_le_bytes(wav[20..22].try_into().unwrap()), 1);
        assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 1);
        // Sample rate and byte rate.
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 11_025);
        assert_eq!(u32::from_le_bytes(wav[28..32].try_into().unwrap()), 22_050);
        // Block align and bits per sample.
        assert_eq!(u16::from_le_bytes(wav[32..34].try_into().unwrap()), 2);
        assert_eq!(u16::from_le_bytes(wav[34..36].try_into().unwrap()), 16);
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 8);

        let data: Vec<i16> = (0..samples.len())
            .map(|i| i16::from_le_bytes([wav[44 + 2 * i], wav[45 + 2 * i]]))
            .collect();
        assert_eq!(data, samples);
    }

    #[test]
    fn test_wav_from_pcm_ignores_half_a_sample() {
        let wav = wav_from_pcm(&[1, 2, 3]);
        assert_eq!(wav.len(), 44 + 2);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 2);
    }

    #[test]
    fn test_wav_from_pcm_empty() {
        let wav = wav_from_pcm(&[]);
        assert_eq!(wav.len(), 44);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 0);
    }

    /// The clip must be something the bot's Songbird build can decode, which depends on
    /// the `wav` and `pcm` Symphonia features.
    #[tokio::test]
    async fn test_wav_is_playable_by_songbird() {
        use songbird::input::{
            Input,
            codecs::{get_codec_registry, get_probe},
        };

        // Half a second of a 440 Hz tone.
        let pcm: Vec<u8> = (0..SAMPLE_RATE / 2)
            .map(|i| {
                let t = f64::from(i) / f64::from(SAMPLE_RATE);
                ((t * 440.0 * std::f64::consts::TAU).sin() * 8_000.0) as i16
            })
            .flat_map(|sample| sample.to_ne_bytes())
            .collect();

        let input = Input::from(wav_from_pcm(&pcm))
            .make_playable_async(get_codec_registry(), get_probe())
            .await
            .expect("Songbird should parse the WAV clip");

        assert!(input.is_playable());

        let parsed = input.parsed().expect("parsed input");
        let track = parsed
            .format
            .default_track()
            .expect("the WAV should have an audio track");
        assert_eq!(track.codec_params.sample_rate, Some(SAMPLE_RATE));
        assert_eq!(
            track.codec_params.n_frames,
            Some(u64::from(SAMPLE_RATE / 2))
        );
        assert_eq!(track.codec_params.channels.map(|c| c.count()), Some(1));
    }

    #[tokio::test]
    async fn test_synthesize_reports_missing_dectalk() {
        let config = config_for("/nonexistent/dectalk/say");

        assert!(!config.is_installed());
        assert_eq!(
            synthesize(&config, "hello").await,
            Err(TtsError::NotInstalled)
        );
        assert_eq!(
            TtsError::NotInstalled.to_string(),
            "Text-to-speech is unavailable: DECtalk is not installed on the bot's server."
        );
    }

    #[tokio::test]
    async fn test_synthesize_reports_missing_dectalk_for_a_directory() {
        let config = config_for("/");

        assert!(!config.is_installed());
        assert_eq!(
            synthesize(&config, "hello").await,
            Err(TtsError::NotInstalled)
        );
    }

    #[cfg(unix)]
    mod privilege_drop {
        use super::*;

        const PASSWD: &str = "root:x:0:0:root:/root:/bin/bash\n\
            daemon:x:1:1:daemon:/usr/sbin:/usr/sbin/nologin\n\
            toor:x:0:0:second root:/root:/bin/sh\n\
            tts:x:993:990::/nonexistent:/usr/sbin/nologin\n\
            nobody:x:65534:65534:nobody:/nonexistent:/usr/sbin/nologin\n";

        #[test]
        fn test_lookup_user() {
            assert_eq!(
                lookup_user(PASSWD, "nobody"),
                Some(RunAs {
                    uid: 65_534,
                    gid: 65_534
                })
            );
            assert_eq!(
                lookup_user(PASSWD, "tts"),
                Some(RunAs { uid: 993, gid: 990 })
            );
            assert_eq!(lookup_user(PASSWD, "nobod"), None);
            assert_eq!(lookup_user(PASSWD, ""), None);
            assert_eq!(lookup_user("garbage\nnobody:x:abc:1\n", "nobody"), None);
        }

        #[test]
        fn test_non_root_bot_does_not_switch_user() {
            assert_eq!(resolve_run_as(1_000, "nobody", PASSWD), None);
            assert_eq!(resolve_run_as(1_000, "tts", PASSWD), None);
        }

        #[test]
        fn test_root_bot_switches_to_configured_user() {
            assert_eq!(
                resolve_run_as(0, "nobody", PASSWD),
                Some(RunAs {
                    uid: 65_534,
                    gid: 65_534
                })
            );
            assert_eq!(
                resolve_run_as(0, "tts", PASSWD),
                Some(RunAs { uid: 993, gid: 990 })
            );
        }

        #[test]
        fn test_root_bot_falls_back_when_user_is_unknown() {
            let fallback = Some(RunAs {
                uid: 65_534,
                gid: 65_534,
            });
            assert_eq!(resolve_run_as(0, "no-such-user", PASSWD), fallback);
            assert_eq!(resolve_run_as(0, "nobody", ""), fallback);
        }

        #[test]
        fn test_root_account_opts_out_of_switching() {
            assert_eq!(resolve_run_as(0, "root", PASSWD), None);
            assert_eq!(resolve_run_as(0, "toor", PASSWD), None);
        }

        /// Runs only as root: checks that the child really loses its privileges.
        #[tokio::test]
        async fn test_child_runs_unprivileged_when_bot_is_root() {
            // SAFETY: `geteuid` only reads the process credentials and cannot fail.
            if unsafe { libc::geteuid() } != 0 {
                eprintln!("skipped: not running as root");
                return;
            }

            let _guard = PROCESS_LOCK.lock().await;

            // A shell stands in for `say`, confined the same way.
            let config = DectalkConfig {
                say_path: PathBuf::from("/usr/bin/say"),
                run_as: resolve_run_as(0, "nobody", PASSWD),
            };
            let mut command = Command::new("/bin/sh");
            command.arg("-c").arg("id -u; id -g; id -G; pwd");
            confine(&mut command, &config);

            let output = command.output().await.expect("run /bin/sh");

            // User, group, supplementary groups (none besides the group), working directory.
            assert_eq!(
                String::from_utf8_lossy(&output.stdout),
                "65534\n65534\n65534\n/usr/bin\n"
            );
        }
    }

    /// Tests that run a small shell script in place of `say`, exercising the real
    /// process handling without DECtalk.
    #[cfg(unix)]
    mod fake_say {
        use super::*;
        use std::os::unix::fs::PermissionsExt;
        use std::sync::atomic::{AtomicUsize, Ordering};

        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        /// A fake `say` script in its own temporary directory, removed on drop.
        struct FakeSay {
            dir: PathBuf,
            config: DectalkConfig,
        }

        impl FakeSay {
            fn new(script: &str) -> Self {
                let dir = env::temp_dir().join(format!(
                    "rusty-tts-test-{}-{}",
                    std::process::id(),
                    COUNTER.fetch_add(1, Ordering::SeqCst)
                ));
                std::fs::create_dir_all(&dir).unwrap();

                let say_path = dir.join("say");
                std::fs::write(&say_path, format!("#!/bin/sh\n{script}\n")).unwrap();
                std::fs::set_permissions(&say_path, std::fs::Permissions::from_mode(0o755))
                    .unwrap();

                Self {
                    dir,
                    config: DectalkConfig {
                        say_path,
                        run_as: None,
                    },
                }
            }

            async fn run(&self, text: &str) -> TtsResult<SayOutput> {
                run_say(&self.config, text, Duration::from_secs(10), MAX_PCM_BYTES).await
            }
        }

        impl Drop for FakeSay {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.dir);
            }
        }

        #[tokio::test]
        async fn test_text_is_passed_on_stdin_unchanged() {
            let _guard = PROCESS_LOCK.lock().await;
            let fake = FakeSay::new("cat");

            // Option-like text and shell syntax arrive as plain bytes.
            let text = "-h -fo /tmp/x; $(id) `id` \"' [:nh] ok!";
            assert_eq!(text.len() % 2, 0, "whole samples only");
            let output = fake.run(text).await.unwrap();

            assert_eq!(output.pcm, text.as_bytes());
            assert!(!output.cut_off);
        }

        #[tokio::test]
        async fn test_arguments_do_not_contain_the_text() {
            let _guard = PROCESS_LOCK.lock().await;
            let fake = FakeSay::new("cat >/dev/null; printf '%s|' \"$@\"");

            let output = fake.run("secret text").await.unwrap();

            assert_eq!(
                String::from_utf8_lossy(&output.pcm),
                "-pre|[:phoneme on]|-fo|stdout:raw|-|"
            );
        }

        #[tokio::test]
        async fn test_environment_is_minimal() {
            let _guard = PROCESS_LOCK.lock().await;
            // Prints the variable names on one line, then the working directory.
            let fake =
                FakeSay::new("cat >/dev/null; env | cut -d= -f1 | sort | tr '\\n' ' '; pwd; pwd");

            let output = fake.run("x").await.unwrap();
            let printed = String::from_utf8_lossy(&output.pcm).into_owned();
            let (names, working_dirs) = printed.split_once('/').unwrap();

            // The shell adds a few of its own; nothing of the bot's environment may appear.
            for name in names.split_whitespace() {
                assert!(
                    ["LD_LIBRARY_PATH", "PATH", "PWD", "OLDPWD", "SHLVL", "_"].contains(&name),
                    "unexpected environment variable {name}"
                );
            }
            assert!(names.contains("LD_LIBRARY_PATH"));

            let working_dir = format!("/{}", working_dirs.lines().next().unwrap());
            assert_eq!(PathBuf::from(working_dir), fake.dir.canonicalize().unwrap());
        }

        #[tokio::test]
        async fn test_non_zero_exit_is_an_error() {
            let _guard = PROCESS_LOCK.lock().await;
            let fake = FakeSay::new("cat >/dev/null; printf 'abcd'; echo 'it broke' >&2; exit 3");

            assert_eq!(fake.run("x").await, Err(TtsError::SynthesisFailed));
        }

        #[tokio::test]
        async fn test_empty_output_is_an_error() {
            let _guard = PROCESS_LOCK.lock().await;
            let fake = FakeSay::new("cat >/dev/null; exit 0");

            assert_eq!(fake.run("x").await, Err(TtsError::NoAudio));
        }

        #[tokio::test]
        async fn test_early_exit_without_reading_stdin_is_an_error() {
            let _guard = PROCESS_LOCK.lock().await;
            let fake = FakeSay::new("exit 127");

            assert_eq!(fake.run("x").await, Err(TtsError::SynthesisFailed));
        }

        #[tokio::test]
        async fn test_slow_process_is_killed_after_the_timeout() {
            let _guard = PROCESS_LOCK.lock().await;
            let fake = FakeSay::new("echo $$ > \"$(dirname \"$0\")/pid\"; exec sleep 30");
            let timeout = Duration::from_millis(500);

            let started = std::time::Instant::now();
            let result = run_say(&fake.config, "x", timeout, MAX_PCM_BYTES).await;

            assert_eq!(result, Err(TtsError::Timeout(timeout)));
            assert!(started.elapsed() < Duration::from_secs(5));

            // The process is gone, not left sleeping.
            if let Ok(pid) = std::fs::read_to_string(fake.dir.join("pid")) {
                assert!(!Path::new("/proc").join(pid.trim()).exists());
            }
        }

        #[tokio::test]
        async fn test_long_audio_is_cut_and_the_process_stopped() {
            let _guard = PROCESS_LOCK.lock().await;
            // Produces audio forever; only the cap stops it.
            let fake = FakeSay::new("cat >/dev/null; exec cat /dev/zero");

            let started = std::time::Instant::now();
            let output = fake.run("x").await.unwrap();

            assert!(output.cut_off);
            assert_eq!(output.pcm.len(), MAX_PCM_BYTES);
            assert!(started.elapsed() < Duration::from_secs(5));
        }

        #[tokio::test]
        async fn test_synthesize_wraps_output_in_wav() {
            let _guard = PROCESS_LOCK.lock().await;
            // One second of silence.
            let fake = FakeSay::new("cat >/dev/null; head -c 22050 /dev/zero");

            let clip = synthesize(&fake.config, "x").await.unwrap();

            assert_eq!(clip.duration, Duration::from_secs(1));
            assert!(!clip.cut_off);
            assert_eq!(clip.wav.len(), 44 + 22_050);
            assert_eq!(&clip.wav[0..4], b"RIFF");
        }

        #[tokio::test]
        async fn test_stderr_flood_does_not_block() {
            let _guard = PROCESS_LOCK.lock().await;
            let fake = FakeSay::new("cat >/dev/null; head -c 300000 /dev/zero >&2; printf 'abcd'");

            assert_eq!(fake.run("x").await.unwrap().pcm, b"abcd");
        }
    }

    /// Runs the real synthesizer. Needs DECtalk at `DECTALK_PATH` (or the default path):
    /// `DECTALK_PATH=/opt/dectalk/say cargo test -- --ignored dectalk_integration`
    #[tokio::test]
    #[ignore = "requires DECtalk to be installed"]
    async fn dectalk_integration() {
        let config = DectalkConfig::from_env();
        assert!(
            config.is_installed(),
            "DECtalk not found at {}",
            config.say_path.display()
        );
        eprintln!(
            "DECtalk at {}, run as {:?}",
            config.say_path.display(),
            config.run_as
        );

        // Plain speech.
        let plain = synthesize(&config, "hello world").await.unwrap();
        assert_eq!(&plain.wav[0..4], b"RIFF");
        assert!(plain.duration > Duration::from_millis(300));
        assert!(plain.duration < Duration::from_secs(10));
        assert!(!plain.cut_off);
        // Not silence.
        assert!(plain.wav[44..].iter().any(|&b| b != 0));

        // Voice, rate and tone commands, and a phoneme string, each produce audio.
        for text in [
            "[:nh] hello [:np] there [:dv ap 200] ok [:rate 300] fast",
            "[:t430,500]",
            "[dah<600,20>]",
        ] {
            let clip = synthesize(&config, text).await.unwrap();
            assert!(clip.duration > Duration::from_millis(200), "{text}");
        }

        // The phoneme prefix is on: a sung note lasts about as long as asked (600 ms),
        // rather than the text being spelled out.
        let sung = synthesize(&config, "[dah<600,20>]").await.unwrap();
        assert!(sung.duration < Duration::from_secs(3));

        // A voice command changes the audio.
        let harry = synthesize(&config, "[:nh] hello world").await.unwrap();
        assert_ne!(harry.wav, plain.wav);

        // Text that looks like an option is spoken, not parsed.
        for text in ["-h", "-fo /tmp/rusty-tts-should-not-exist.wav", "-"] {
            let clip = synthesize(&config, text).await.unwrap();
            assert!(clip.duration > Duration::from_millis(200), "{text}");
        }
        assert!(!Path::new("/tmp/rusty-tts-should-not-exist.wav").exists());

        // Long text is cut at the cap.
        let long_text = "aeiou ".repeat(600);
        let long = synthesize(&config, long_text.trim()).await.unwrap();
        assert!(long.cut_off);
        assert_eq!(
            long.duration,
            Duration::from_secs(u64::from(MAX_AUDIO_SECONDS))
        );

        // Text with nothing to pronounce gives a clear error.
        assert_eq!(synthesize(&config, "[:nh]").await, Err(TtsError::NoAudio));

        // The audio is accepted by Songbird's decoder.
        let input = songbird::input::Input::from(plain.wav.clone())
            .make_playable_async(
                songbird::input::codecs::get_codec_registry(),
                songbird::input::codecs::get_probe(),
            )
            .await
            .expect("Songbird should parse DECtalk audio");
        assert!(input.is_playable());
    }
}
