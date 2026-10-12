//! Cut, shrink or synthesize AVI videos without FFmpeg.
use crate::{
    avi::{self, AviReader, AviWriter, VideoFormat},
    synth::{Scene, SceneOptions},
};
use anyhow::{Context, Result, ensure};
use std::path::Path;

#[derive(Clone, Debug)]
pub struct ClipOptions {
    /// Seconds to skip from the start of the input.
    pub start: f64,
    /// Seconds to keep; `None` keeps the rest of the video.
    pub duration: Option<f64>,
    pub format: VideoFormat,
    pub quality: u8,
    /// Keep every Nth frame (1 keeps all); the output frame rate is divided by N.
    pub every: u32,
}
impl Default for ClipOptions {
    fn default() -> Self {
        Self {
            start: 0.0,
            duration: None,
            format: VideoFormat::Mjpeg,
            quality: 90,
            every: 1,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ClipSummary {
    pub frames: u64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub bytes: u64,
}
/// Copy part of an AVI into a new AVI, optionally re-encoding it smaller.
pub fn clip(input: &Path, output: &Path, options: &ClipOptions) -> Result<ClipSummary> {
    ensure!(
        options.start.is_finite() && options.start >= 0.0,
        "--start must be zero or positive"
    );
    ensure!(
        options.duration.is_none_or(|d| d.is_finite() && d > 0.0),
        "--duration must be positive"
    );
    ensure!(options.every >= 1, "--every must be at least 1");
    ensure!(
        !output.exists(),
        "{} already exists; choose a new output name",
        output.display()
    );
    if !avi::is_avi(input) {
        return clip_with_ffmpeg(input, output, options);
    }
    let mut reader = AviReader::open(input)?;
    let info = reader.info().clone();
    let skip = (options.start * info.fps).round() as u64;
    ensure!(
        reader.skip(skip)? == skip,
        "The video is shorter than --start {}s",
        options.start
    );
    let limit = options
        .duration
        .map(|d| (d * info.fps).round().max(1.0) as u64);
    let mut chunk = Vec::new();
    let mut read = 0u64;
    let size = (info.width, info.height, info.rate, info.scale);
    write_clip(output, size, options, |pixels| {
        if limit.is_some_and(|n| read >= n) {
            return Ok(false);
        }
        read += 1;
        reader.next_frame(&mut chunk, pixels)
    })
}
/// Encode frames from `next` into a new AVI, keeping every Nth. A failed
/// clip leaves no partial file behind.
fn write_clip(
    output: &Path,
    (width, height, rate, scale): (u32, u32, u32, u32),
    options: &ClipOptions,
    mut next: impl FnMut(&mut Vec<u8>) -> Result<bool>,
) -> Result<ClipSummary> {
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let mut writer = AviWriter::create(
        output,
        width,
        height,
        rate,
        scale.saturating_mul(options.every),
        options.format,
    )?;
    let result = (|| {
        let (mut pixels, mut encoded) = (Vec::new(), Vec::new());
        let mut read = 0u64;
        while next(&mut pixels)? {
            if read.is_multiple_of(options.every as u64) {
                avi::encode(
                    options.format,
                    options.quality,
                    true,
                    width,
                    height,
                    &pixels,
                    &mut encoded,
                )?;
                ensure!(
                    writer.size_after(encoded.len()) < 1900 * 1024 * 1024,
                    "Output would exceed the 1.9 GiB AVI limit; use a shorter --duration or format mjpeg"
                );
                writer.write(&encoded)?;
            }
            read += 1;
        }
        ensure!(writer.frames() > 0, "No frames in the selected range");
        writer.finish()
    })();
    let frames = writer.frames() as u64;
    drop(writer);
    if let Err(error) = result {
        let _ = std::fs::remove_file(output);
        return Err(error);
    }
    Ok(ClipSummary {
        frames,
        width,
        height,
        fps: rate as f64 / scale as f64 / options.every as f64,
        bytes: std::fs::metadata(output)?.len(),
    })
}
/// Any format FFmpeg reads. FFmpeg only decodes, to gray Y4M on a pipe;
/// Rekkefly encodes, so `--quality` means the same for every input. The
/// time range is cut after decoding: exact, and an input-side seek can drop
/// frames from phone and screen videos with irregular timestamps.
fn clip_with_ffmpeg(input: &Path, output: &Path, options: &ClipOptions) -> Result<ClipSummary> {
    use std::io::{BufRead, BufReader, Read};
    let mut command = std::process::Command::new(crate::source::ffmpeg_executable());
    command
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-i"])
        .arg(input);
    if options.start > 0.0 {
        command.arg("-ss").arg(options.start.to_string());
    }
    if let Some(duration) = options.duration {
        command.arg("-t").arg(duration.to_string());
    }
    command
        .args(["-map", "0:v:0", "-an", "-sn", "-dn", "-pix_fmt", "gray"])
        .args(["-f", "yuv4mpegpipe", "pipe:1"])
        .stdout(std::process::Stdio::piped());
    let mut child = command
        .spawn()
        .context("This input is not an AVI; FFmpeg is needed to read it. Install FFmpeg")?;
    let mut stdout = BufReader::new(child.stdout.take().context("FFmpeg gave no output")?);
    let mut line = Vec::new();
    let header = stdout
        .read_until(b'\n', &mut line)
        .map_err(anyhow::Error::from)
        .and_then(|_| Y4m::parse(&line));
    let decoded = header.is_ok();
    let result = header.and_then(|y4m| {
        let size = (y4m.width as usize) * (y4m.height as usize);
        write_clip(
            output,
            (y4m.width, y4m.height, y4m.rate, y4m.scale),
            options,
            |pixels| {
                line.clear();
                if stdout.read_until(b'\n', &mut line)? == 0 {
                    return Ok(false);
                }
                ensure!(line.starts_with(b"FRAME"), "Unexpected data from FFmpeg");
                pixels.resize(size, 0);
                stdout.read_exact(pixels)?;
                Ok(true)
            },
        )
    });
    if result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    match result {
        Err(error) if !decoded => Err(error.context(format!(
            "FFmpeg could not read {} ({status})",
            input.display()
        ))),
        Err(error) => Err(error),
        Ok(_) if !status.success() => {
            let _ = std::fs::remove_file(output);
            anyhow::bail!("FFmpeg failed with {status}")
        }
        Ok(summary) => Ok(summary),
    }
}
/// The stream header of a gray YUV4MPEG2 video.
#[derive(Debug, PartialEq)]
struct Y4m {
    width: u32,
    height: u32,
    rate: u32,
    scale: u32,
}
impl Y4m {
    fn parse(line: &[u8]) -> Result<Self> {
        let text = std::str::from_utf8(line).context("Unexpected data from FFmpeg")?;
        let mut words = text.split_ascii_whitespace();
        ensure!(
            words.next() == Some("YUV4MPEG2"),
            "FFmpeg produced no video"
        );
        let (mut width, mut height, mut rate, mut scale, mut mono) = (0, 0, 0, 0, false);
        for word in words {
            let value = &word[1..];
            match word.as_bytes()[0] {
                b'W' => width = value.parse()?,
                b'H' => height = value.parse()?,
                b'F' => {
                    let (n, d) = value.split_once(':').context("Y4M frame rate")?;
                    (rate, scale) = (n.parse()?, d.parse()?);
                }
                b'C' => mono = value.starts_with("mono"),
                _ => {}
            }
        }
        ensure!(
            width > 0 && height > 0 && mono,
            "FFmpeg did not decode to gray video"
        );
        if rate == 0 || scale == 0 {
            (rate, scale) = (30, 1);
        }
        Ok(Self {
            width,
            height,
            rate,
            scale,
        })
    }
}
/// Render a synthetic tethered-fly video with known wing angles.
pub fn synthesize(
    output: &Path,
    seconds: f64,
    fps: u32,
    width: u32,
    height: u32,
    quality: u8,
    options: SceneOptions,
) -> Result<ClipSummary> {
    ensure!(
        seconds > 0.0 && fps > 0,
        "Duration and fps must be positive"
    );
    ensure!(
        !output.exists(),
        "{} already exists; choose a new output name",
        output.display()
    );
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let scene = Scene::new(width, height, options);
    let mut writer = AviWriter::create(output, width, height, fps, 1, VideoFormat::Mjpeg)?;
    let frames = (seconds * fps as f64).round() as u64;
    let mut encoded = Vec::new();
    for k in 0..frames {
        let frame = scene.render(k as f32 / fps as f32, k);
        avi::encode(
            VideoFormat::Mjpeg,
            quality,
            true,
            width,
            height,
            &frame.pixels,
            &mut encoded,
        )?;
        writer.write(&encoded)?;
    }
    writer.finish()?;
    drop(writer);
    Ok(ClipSummary {
        frames,
        width,
        height,
        fps: fps as f64,
        bytes: std::fs::metadata(output)?.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clip_cuts_a_time_range_and_thins_frames() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("long.avi");
        let mut writer = AviWriter::create(&source, 40, 30, 10, 1, VideoFormat::Raw).unwrap();
        for k in 0..50u8 {
            writer.write(&vec![k * 4; 40 * 30]).unwrap();
        }
        writer.finish().unwrap();
        drop(writer);
        let out = dir.path().join("short.avi");
        let summary = clip(
            &source,
            &out,
            &ClipOptions {
                start: 1.0,
                duration: Some(2.0),
                format: VideoFormat::Png,
                every: 2,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(summary.frames, 10);
        assert_eq!(summary.fps, 5.0);
        let mut reader = AviReader::open(&out).unwrap();
        let (mut c, mut p) = (Vec::new(), Vec::new());
        for k in 0..10u8 {
            assert!(reader.next_frame(&mut c, &mut p).unwrap());
            assert_eq!(p[0], (10 + 2 * k) * 4);
        }
        assert!(
            clip(&source, &out, &ClipOptions::default()).is_err(),
            "never overwrite"
        );
        let late = ClipOptions {
            start: 60.0,
            ..Default::default()
        };
        assert!(clip(&source, &dir.path().join("late.avi"), &late).is_err());
    }
    #[test]
    fn y4m_headers_give_size_rate_and_require_gray() {
        let header = Y4m::parse(b"YUV4MPEG2 W640 H480 F30000:1001 Ip A1:1 Cmono XYSCSS=MONO\n");
        assert_eq!(
            header.unwrap(),
            Y4m {
                width: 640,
                height: 480,
                rate: 30000,
                scale: 1001
            }
        );
        assert!(Y4m::parse(b"YUV4MPEG2 W64 H48 F25:1 C420jpeg\n").is_err());
        assert!(Y4m::parse(b"").is_err());
    }
    /// Phone and screen videos (MP4, MOV, M4V) go through FFmpeg. Skipped
    /// where FFmpeg is not installed.
    #[test]
    fn ffmpeg_inputs_keep_every_frame_of_the_range() {
        let dir = tempfile::tempdir().unwrap();
        let mp4 = dir.path().join("phone.mp4");
        let made = std::process::Command::new(crate::source::ffmpeg_executable())
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=64x48:rate=10",
            ])
            .args(["-t", "3", "-c:v", "mpeg4", "-bf", "2"])
            .arg(&mp4)
            .status();
        if !made.is_ok_and(|s| s.success()) {
            eprintln!("FFmpeg unavailable; skipped");
            return;
        }
        let out = dir.path().join("cut.avi");
        let options = ClipOptions {
            start: 1.0,
            duration: Some(1.5),
            quality: 80,
            ..Default::default()
        };
        let summary = clip(&mp4, &out, &options).unwrap();
        assert_eq!(
            (summary.frames, summary.width, summary.height),
            (15, 64, 48)
        );
        assert_eq!(summary.fps, 10.0);
        let mut reader = AviReader::open(&out).unwrap();
        let (mut c, mut p) = (Vec::new(), Vec::new());
        let mut frames = 0;
        while reader.next_frame(&mut c, &mut p).unwrap() {
            frames += 1;
        }
        assert_eq!(frames, 15, "no repeated or dropped frames");
        let missing = dir.path().join("missing.mp4");
        let error = clip(&missing, &dir.path().join("x.avi"), &options).unwrap_err();
        assert!(
            format!("{error:#}").contains("FFmpeg could not read"),
            "{error:#}"
        );
        assert!(!dir.path().join("x.avi").exists(), "no partial output");
    }
    #[test]
    fn synthesized_sample_replays_natively() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("sample.avi");
        let summary = synthesize(&out, 0.5, 20, 320, 240, 80, SceneOptions::default()).unwrap();
        assert_eq!(summary.frames, 10);
        assert!(summary.bytes < 10 * 320 * 240 / 4);
        let mut reader = AviReader::open(&out).unwrap();
        assert_eq!(reader.info().fps, 20.0);
        let (mut c, mut p) = (Vec::new(), Vec::new());
        assert!(reader.next_frame(&mut c, &mut p).unwrap());
        assert_eq!(p.len(), 320 * 240);
    }
}
