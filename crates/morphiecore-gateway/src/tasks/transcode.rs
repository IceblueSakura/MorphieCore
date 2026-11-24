//! Bounded FFmpeg transcoding for static image outputs, speech outputs, and untrusted ASR audio uploads.
use crate::{
    ApiError,
    semantic::{
        task::{
            image_generation::{GeneratedImage, ImageData},
            speech_recognition::{AudioInput, InputFormat, MAX_INPUT_BYTES},
        },
        value::{AudioArtifact, AudioEncoding, ImageFormat, MAX_AUDIO_BYTES, Presence},
    },
};
use axum::http::StatusCode;
use base64::{Engine, engine::general_purpose::STANDARD};
use bytes::Bytes;
use std::{ffi::OsString, path::Path, process::Stdio};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::Semaphore,
};

const MAX_AUDIO_SECONDS: &str = "600";
static IMAGE_WORKERS: Semaphore = Semaphore::const_new(2);
static AUDIO_WORKERS: Semaphore = Semaphore::const_new(2);

enum TranscodeFailure {
    OverLimit,
    BadInput,
    Upstream,
}

fn unavailable() -> ApiError {
    ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        crate::error::ErrorKind::Unavailable,
    )
}

async fn run_ffmpeg(
    executable: &Path,
    workers: &Semaphore,
    args: &[OsString],
    stdin_bytes: Option<&[u8]>,
    limit: usize,
) -> Result<Vec<u8>, TranscodeFailure> {
    let _worker = workers
        .acquire()
        .await
        .map_err(|_| TranscodeFailure::Upstream)?;
    let mut command = Command::new(executable);
    command
        .env_clear()
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-xerror",
            "-max_alloc",
            "67108864",
            "-filter_threads",
            "1",
            "-threads",
            "1",
        ])
        .args(args)
        .stdin(if stdin_bytes.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(target_os = "linux")]
    // SAFETY: the pre-exec callback calls only setrlimit and returns its errno.
    unsafe {
        command.pre_exec(|| {
            let bytes = 512 << 20;
            let bound = libc::rlimit {
                rlim_cur: bytes,
                rlim_max: bytes,
            };
            if libc::setrlimit(libc::RLIMIT_AS, &bound) == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        });
    }
    let mut child = command.spawn().map_err(|_| TranscodeFailure::Upstream)?;
    let stdin = child.stdin.take();
    let stdout = child.stdout.take().expect("piped");
    let mut bytes = Vec::new();
    let mut over_limit = false;

    let write_task = async {
        if let (Some(mut pipe), Some(input)) = (stdin, stdin_bytes) {
            pipe.write_all(input).await?;
            drop(pipe);
        }
        Ok::<_, std::io::Error>(())
    };
    let read_task = async {
        stdout
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .await?;
        if bytes.len() > limit {
            over_limit = true;
            return Err(std::io::Error::other("transcode limit"));
        }
        Ok(())
    };

    let wait = tokio::try_join!(write_task, read_task, child.wait());
    let Ok(((), (), status)) = wait else {
        let _ = child.kill().await;
        return Err(if over_limit {
            TranscodeFailure::OverLimit
        } else {
            TranscodeFailure::BadInput
        });
    };
    if !status.success() {
        return Err(TranscodeFailure::BadInput);
    }
    Ok(bytes)
}

fn push_args(args: &mut Vec<OsString>, items: &[&str]) {
    args.extend(items.iter().map(OsString::from));
}

pub(crate) async fn image(
    executable: Option<&Path>,
    image: &mut GeneratedImage,
    target: ImageFormat,
    compression: Option<u8>,
    limit: usize,
) -> Result<(), ApiError> {
    if image.format.value() == Some(&target) && compression.is_none() {
        return Ok(());
    }
    let (demuxer, decoder) = match image.format.value() {
        Some(ImageFormat::Png) => ("png_pipe", "png"),
        Some(ImageFormat::Jpeg) => ("jpeg_pipe", "mjpeg"),
        Some(ImageFormat::Webp) => ("webp_pipe", "webp"),
        _ => return Err(ApiError::upstream()),
    };
    if image.data.decoded_bytes() > limit {
        return Err(ApiError::upstream());
    }
    let executable = executable.ok_or_else(unavailable)?;
    let input = STANDARD
        .decode(image.data.as_base64())
        .map_err(|_| ApiError::upstream())?;
    let mut args = Vec::new();
    push_args(
        &mut args,
        &[
            "-max_pixels",
            "16777216",
            "-f",
            demuxer,
            "-c:v",
            decoder,
            "-protocol_whitelist",
            "pipe",
            "-i",
            "pipe:0",
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-map_metadata",
            "-1",
            "-threads",
            "1",
        ],
    );
    let q_str;
    match target {
        ImageFormat::Png => push_args(&mut args, &["-c:v", "png", "-f", "image2pipe", "pipe:1"]),
        ImageFormat::Jpeg => {
            let quality = u32::from(compression.unwrap_or(85).min(100));
            let q = (31 - ((quality * 29 + 50) / 100)).clamp(2, 31);
            q_str = q.to_string();
            push_args(
                &mut args,
                &[
                    "-pix_fmt",
                    "yuvj420p",
                    "-c:v",
                    "mjpeg",
                    "-q:v",
                    &q_str,
                    "-f",
                    "image2pipe",
                    "pipe:1",
                ],
            );
        }
        ImageFormat::Webp => {
            let quality = compression.unwrap_or(80).min(100);
            q_str = quality.to_string();
            push_args(
                &mut args,
                &[
                    "-c:v", "libwebp", "-quality", &q_str, "-f", "webp", "pipe:1",
                ],
            );
        }
        ImageFormat::Gif | ImageFormat::Bmp => return Err(ApiError::invalid()),
    }
    let bytes = run_ffmpeg(executable, &IMAGE_WORKERS, &args, Some(&input), limit)
        .await
        .map_err(|_| ApiError::upstream())?;
    let valid_magic = match target {
        ImageFormat::Png => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        ImageFormat::Jpeg => bytes.starts_with(b"\xff\xd8\xff"),
        ImageFormat::Webp => {
            bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP"
        }
        ImageFormat::Gif | ImageFormat::Bmp => return Err(ApiError::invalid()),
    };
    if !valid_magic {
        return Err(ApiError::upstream());
    }
    image.data = ImageData::new(&STANDARD.encode(bytes)).map_err(|_| ApiError::upstream())?;
    image.format = Presence::Value(target);
    Ok(())
}

pub(crate) async fn speech_audio(
    executable: Option<&Path>,
    audio: AudioArtifact,
    target: AudioEncoding,
    limit: usize,
) -> Result<AudioArtifact, ApiError> {
    if audio.encoding() == Some(target) {
        return Ok(audio);
    }
    let bounded_limit = limit.min(MAX_AUDIO_BYTES);
    let source = audio.encoding().ok_or_else(ApiError::upstream)?;
    let executable = executable.ok_or_else(unavailable)?;
    let mut args = Vec::new();
    match source {
        AudioEncoding::Mp3 => push_args(&mut args, &["-f", "mp3"]),
        AudioEncoding::Wav => push_args(&mut args, &["-f", "wav"]),
        AudioEncoding::PcmS16Le24KhzMono => {
            push_args(&mut args, &["-f", "s16le", "-ar", "24000", "-ac", "1"])
        }
        AudioEncoding::Opus => push_args(&mut args, &["-f", "ogg"]),
        AudioEncoding::Aac => push_args(&mut args, &["-f", "aac"]),
        AudioEncoding::Flac => push_args(&mut args, &["-f", "flac"]),
    }
    push_args(
        &mut args,
        &[
            "-protocol_whitelist",
            "pipe",
            "-i",
            "pipe:0",
            "-vn",
            "-sn",
            "-dn",
            "-map",
            "0:a:0",
            "-map_metadata",
            "-1",
            "-t",
            MAX_AUDIO_SECONDS,
        ],
    );
    match target {
        AudioEncoding::Mp3 => push_args(
            &mut args,
            &["-c:a", "libmp3lame", "-b:a", "128k", "-f", "mp3", "pipe:1"],
        ),
        AudioEncoding::Opus => push_args(
            &mut args,
            &["-c:a", "libopus", "-b:a", "64k", "-f", "opus", "pipe:1"],
        ),
        AudioEncoding::Aac => push_args(
            &mut args,
            &["-c:a", "aac", "-b:a", "128k", "-f", "adts", "pipe:1"],
        ),
        AudioEncoding::Flac => {
            push_args(&mut args, &["-c:a", "flac", "-f", "flac", "pipe:1"]);
        }
        AudioEncoding::Wav => {
            push_args(&mut args, &["-c:a", "pcm_s16le", "-f", "wav", "pipe:1"]);
        }
        AudioEncoding::PcmS16Le24KhzMono => push_args(
            &mut args,
            &[
                "-c:a",
                "pcm_s16le",
                "-ar",
                "24000",
                "-ac",
                "1",
                "-f",
                "s16le",
                "pipe:1",
            ],
        ),
    }
    let bytes = run_ffmpeg(
        executable,
        &AUDIO_WORKERS,
        &args,
        Some(audio.data()),
        bounded_limit,
    )
    .await
    .map_err(|_| ApiError::upstream())?;
    if !valid_audio_magic(target, &bytes) {
        return Err(ApiError::upstream());
    }
    AudioArtifact::new(Bytes::from(bytes), Some(target)).map_err(|_| ApiError::upstream())
}

fn valid_audio_magic(target: AudioEncoding, bytes: &[u8]) -> bool {
    match target {
        AudioEncoding::Mp3 => valid_mp3_magic(bytes),
        AudioEncoding::Opus => bytes.starts_with(b"OggS"),
        AudioEncoding::Aac => bytes.len() >= 2 && bytes[0] == 0xFF && (bytes[1] & 0xF0) == 0xF0,
        AudioEncoding::Flac => bytes.starts_with(b"fLaC"),
        AudioEncoding::Wav => {
            bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE"
        }
        AudioEncoding::PcmS16Le24KhzMono => !bytes.is_empty() && bytes.len().is_multiple_of(2),
    }
}

fn valid_mp3_magic(bytes: &[u8]) -> bool {
    bytes.starts_with(b"ID3") || (bytes.len() >= 2 && bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0)
}

pub(crate) async fn transcription_input_to_mp3(
    executable: Option<&Path>,
    input: &AudioInput,
    limit: usize,
) -> Result<AudioInput, ApiError> {
    if matches!(input.format(), InputFormat::Wav | InputFormat::Mp3) {
        return Ok(input.clone());
    }
    let executable = executable.ok_or_else(unavailable)?;
    let bounded_limit = limit.min(MAX_INPUT_BYTES);
    let mut args = Vec::new();
    let mut _temp = None;
    if matches!(input.format(), InputFormat::Mp4 | InputFormat::M4a) {
        let mut temp = tempfile::Builder::new()
            .prefix("morphiecore-asr-")
            .tempfile()
            .map_err(|_| ApiError::upstream())?;
        std::io::Write::write_all(&mut temp, input.data()).map_err(|_| ApiError::upstream())?;
        let path = temp.path().as_os_str().to_owned();
        _temp = Some(temp);
        push_args(
            &mut args,
            &["-protocol_whitelist", "file,pipe", "-f", "mov", "-i"],
        );
        args.push(path);
    } else {
        let demuxer = match input.format() {
            InputFormat::Flac => "flac",
            InputFormat::Ogg => "ogg",
            InputFormat::Webm => "matroska",
            InputFormat::Mpga => "mp3",
            InputFormat::Mpeg => "mpeg",
            InputFormat::Wav | InputFormat::Mp3 | InputFormat::Mp4 | InputFormat::M4a => {
                unreachable!("handled above")
            }
        };
        push_args(
            &mut args,
            &["-protocol_whitelist", "pipe", "-f", demuxer, "-i", "pipe:0"],
        );
    }
    push_args(
        &mut args,
        &[
            "-vn",
            "-sn",
            "-dn",
            "-map",
            "0:a:0",
            "-map_metadata",
            "-1",
            "-t",
            MAX_AUDIO_SECONDS,
            "-c:a",
            "libmp3lame",
            "-b:a",
            "128k",
            "-f",
            "mp3",
            "pipe:1",
        ],
    );
    let stdin_bytes = if _temp.is_some() {
        None
    } else {
        Some(input.data().as_ref())
    };
    let bytes = run_ffmpeg(
        executable,
        &AUDIO_WORKERS,
        &args,
        stdin_bytes,
        bounded_limit,
    )
    .await
    .map_err(|err| match err {
        TranscodeFailure::OverLimit => ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            crate::error::ErrorKind::RequestTooLarge,
        ),
        TranscodeFailure::BadInput => ApiError::invalid(),
        TranscodeFailure::Upstream => ApiError::upstream(),
    })?;
    drop(_temp);
    if !valid_mp3_magic(&bytes) {
        return Err(ApiError::invalid());
    }
    AudioInput::new(Bytes::from(bytes), InputFormat::Mp3).map_err(|_| ApiError::invalid())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn gen_sample(executable: &Path, args: &[&str], limit: usize) -> Vec<u8> {
        let mut command = Command::new(executable);
        command.args(["-v", "error"]).args(args);
        let output = morphiecore_test_support::child_process::run(
            &mut command,
            b"",
            std::time::Duration::from_secs(5),
            limit,
        )
        .await
        .unwrap();
        assert!(output.status.success(), "ffmpeg sample generation failed");
        output.stdout
    }

    #[tokio::test]
    #[ignore = "requires a trusted FFmpeg executable in MORPHIECORE_TEST_FFMPEG"]
    async fn image_formats_preserve_dimensions_and_enforce_budget() {
        let executable =
            std::path::PathBuf::from(std::env::var_os("MORPHIECORE_TEST_FFMPEG").unwrap());
        let jpeg_bytes = gen_sample(
            &executable,
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=32x24:rate=1",
                "-frames:v",
                "1",
                "-threads",
                "1",
                "-q:v",
                "31",
                "-c:v",
                "mjpeg",
                "-f",
                "image2pipe",
                "pipe:1",
            ],
            4096,
        )
        .await;
        assert!(jpeg_bytes.starts_with(b"\xff\xd8\xff"));
        let mut img = GeneratedImage::new(ImageData::new(&STANDARD.encode(&jpeg_bytes)).unwrap());
        img.format = Presence::Value(ImageFormat::Jpeg);

        let mut png = img.clone();
        image(Some(&executable), &mut png, ImageFormat::Png, None, 16384)
            .await
            .unwrap();
        let png_bytes = STANDARD.decode(png.data.as_base64()).unwrap();
        assert_eq!(png.format, Presence::Value(ImageFormat::Png));
        assert_eq!(&png_bytes[16..24], b"\x00\x00\x00\x20\x00\x00\x00\x18");

        let mut webp = png.clone();
        image(
            Some(&executable),
            &mut webp,
            ImageFormat::Webp,
            Some(50),
            16384,
        )
        .await
        .unwrap();
        let webp_bytes = STANDARD.decode(webp.data.as_base64()).unwrap();
        assert_eq!(webp.format, Presence::Value(ImageFormat::Webp));
        assert_eq!(&webp_bytes[8..12], b"WEBP");

        let mut jpg = webp.clone();
        image(
            Some(&executable),
            &mut jpg,
            ImageFormat::Jpeg,
            Some(60),
            16384,
        )
        .await
        .unwrap();
        assert_eq!(jpg.format, Presence::Value(ImageFormat::Jpeg));

        let err = image(
            Some(&executable),
            &mut img.clone(),
            ImageFormat::Png,
            None,
            png_bytes.len() - 1,
        )
        .await
        .unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    #[ignore = "requires a trusted FFmpeg executable in MORPHIECORE_TEST_FFMPEG"]
    async fn speech_audio_converts_all_openai_targets() {
        let executable =
            std::path::PathBuf::from(std::env::var_os("MORPHIECORE_TEST_FFMPEG").unwrap());
        let mp3_bytes = gen_sample(
            &executable,
            &[
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=0.3",
                "-c:a",
                "libmp3lame",
                "-b:a",
                "64k",
                "-f",
                "mp3",
                "pipe:1",
            ],
            65536,
        )
        .await;
        let source = AudioArtifact::new(Bytes::from(mp3_bytes), Some(AudioEncoding::Mp3)).unwrap();
        for target in [
            AudioEncoding::Wav,
            AudioEncoding::Opus,
            AudioEncoding::Aac,
            AudioEncoding::Flac,
            AudioEncoding::PcmS16Le24KhzMono,
        ] {
            let converted = speech_audio(Some(&executable), source.clone(), target, 262_144)
                .await
                .unwrap();
            assert_eq!(converted.encoding(), Some(target));
            assert!(!converted.data().is_empty());
        }
    }

    #[tokio::test]
    #[ignore = "requires a trusted FFmpeg executable in MORPHIECORE_TEST_FFMPEG"]
    async fn transcription_input_converts_containers_and_rejects_corrupt_or_oversized() {
        let executable =
            std::path::PathBuf::from(std::env::var_os("MORPHIECORE_TEST_FFMPEG").unwrap());
        for (format, gen_args) in [
            (
                InputFormat::Flac,
                vec![
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:duration=0.3",
                    "-c:a",
                    "flac",
                    "-f",
                    "flac",
                    "pipe:1",
                ],
            ),
            (
                InputFormat::Ogg,
                vec![
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:duration=0.3",
                    "-c:a",
                    "libopus",
                    "-f",
                    "ogg",
                    "pipe:1",
                ],
            ),
            (
                InputFormat::Webm,
                vec![
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:duration=0.3",
                    "-c:a",
                    "libopus",
                    "-f",
                    "webm",
                    "pipe:1",
                ],
            ),
            (
                InputFormat::Mpga,
                vec![
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:duration=0.3",
                    "-c:a",
                    "libmp3lame",
                    "-f",
                    "mp3",
                    "pipe:1",
                ],
            ),
        ] {
            let bytes = gen_sample(&executable, &gen_args, 65536).await;
            let input = AudioInput::new(Bytes::from(bytes), format).unwrap();
            let mp3 = transcription_input_to_mp3(Some(&executable), &input, MAX_INPUT_BYTES)
                .await
                .unwrap();
            assert_eq!(mp3.format(), InputFormat::Mp3);
        }

        // M4A/MP4 written to a file so the moov atom can sit at EOF (seekable container).
        let temp_m4a = tempfile::Builder::new().suffix(".m4a").tempfile().unwrap();
        let status = Command::new(&executable)
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=0.3",
                "-c:a",
                "aac",
                "-f",
                "ipod",
            ])
            .arg(temp_m4a.path())
            .status()
            .await
            .unwrap();
        assert!(status.success());
        let m4a_bytes = std::fs::read(temp_m4a.path()).unwrap();
        let m4a_input = AudioInput::new(Bytes::from(m4a_bytes), InputFormat::M4a).unwrap();
        let mp3 = transcription_input_to_mp3(Some(&executable), &m4a_input, MAX_INPUT_BYTES)
            .await
            .unwrap();
        assert_eq!(mp3.format(), InputFormat::Mp3);

        let too_small = transcription_input_to_mp3(Some(&executable), &m4a_input, 32)
            .await
            .unwrap_err();
        assert_eq!(too_small.status, StatusCode::PAYLOAD_TOO_LARGE);

        let corrupt =
            AudioInput::new(Bytes::from_static(b"not-valid-ogg"), InputFormat::Ogg).unwrap();
        let bad = transcription_input_to_mp3(Some(&executable), &corrupt, MAX_INPUT_BYTES)
            .await
            .unwrap_err();
        assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    }
}
