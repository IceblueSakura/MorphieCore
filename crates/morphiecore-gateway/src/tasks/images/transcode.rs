//! Convert only complete native JPEG/WebP output, using a trusted local encoder.
use crate::{
    ApiError,
    semantic::{
        task::image_generation::{GeneratedImage, ImageData},
        value::{ImageFormat, Presence},
    },
};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{path::Path, process::Stdio};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::Semaphore,
};

static WORKER: Semaphore = Semaphore::const_new(1);

pub(super) async fn png(
    executable: Option<&Path>,
    image: &mut GeneratedImage,
    limit: usize,
) -> Result<(), ApiError> {
    let (demuxer, decoder) = match image.format.value() {
        Some(ImageFormat::Png) => return Ok(()),
        Some(ImageFormat::Jpeg) => ("jpeg_pipe", "mjpeg"),
        Some(ImageFormat::Webp) => ("webp_pipe", "webp"),
        _ => return Err(ApiError::upstream()),
    };
    if image.data.decoded_bytes() > limit {
        return Err(ApiError::upstream());
    }
    let executable = executable.ok_or_else(ApiError::upstream)?;
    let _worker = WORKER.acquire().await.map_err(|_| ApiError::upstream())?;
    let input = STANDARD
        .decode(image.data.as_base64())
        .map_err(|_| ApiError::upstream())?;
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
            "-c:v",
            "png",
            "-f",
            "image2pipe",
            "pipe:1",
        ])
        .stdin(Stdio::piped())
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
    let mut child = command.spawn().map_err(|_| ApiError::upstream())?;
    let mut stdin = child.stdin.take().expect("piped");
    let stdout = child.stdout.take().expect("piped");
    let mut bytes = Vec::new();
    let wait = tokio::try_join!(
        async move {
            stdin.write_all(&input).await?;
            drop(stdin);
            Ok(())
        },
        async {
            stdout
                .take(limit as u64 + 1)
                .read_to_end(&mut bytes)
                .await?;
            if bytes.len() <= limit {
                Ok(())
            } else {
                Err(std::io::Error::other("image output limit"))
            }
        },
        child.wait(),
    );
    let Ok(((), (), status)) = wait else {
        let _ = child.kill().await;
        return Err(ApiError::upstream());
    };
    if !status.success() || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(ApiError::upstream());
    }
    image.data = ImageData::new(&STANDARD.encode(bytes)).map_err(|_| ApiError::upstream())?;
    image.format = Presence::Value(ImageFormat::Png);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires a trusted FFmpeg executable in MORPHIECORE_TEST_IMAGE_FFMPEG"]
    async fn jpeg_to_png_preserves_dimensions_and_never_publishes_over_budget() {
        let executable =
            std::path::PathBuf::from(std::env::var_os("MORPHIECORE_TEST_IMAGE_FFMPEG").unwrap());
        let mut source = Command::new(&executable);
        source.args([
            "-v",
            "error",
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
        ]);
        let source = morphiecore_test_support::child_process::run(
            &mut source,
            b"",
            std::time::Duration::from_secs(5),
            4096,
        )
        .await
        .unwrap();
        assert!(source.status.success() && source.stdout.starts_with(b"\xff\xd8\xff"));
        let mut image =
            GeneratedImage::new(ImageData::new(&STANDARD.encode(source.stdout)).unwrap());
        image.format = Presence::Value(ImageFormat::Jpeg);
        let mut converted = image.clone();
        png(Some(&executable), &mut converted, 4096).await.unwrap();
        let result = STANDARD.decode(converted.data.as_base64()).unwrap();
        assert_eq!(converted.format, Presence::Value(ImageFormat::Png));
        assert_eq!(&result[..16], b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR");
        assert_eq!(&result[16..24], b"\x00\x00\x00\x20\x00\x00\x00\x18");
        assert!(
            result.len() > image.data.decoded_bytes(),
            "exercise the stdout cap"
        );
        assert!(
            png(Some(&executable), &mut image, result.len() - 1)
                .await
                .is_err()
        );
        assert_eq!(image.format, Presence::Value(ImageFormat::Jpeg));
    }
}
