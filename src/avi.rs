//! Minimal RIFF AVI reader and writer for grayscale video, without FFmpeg.
//!
//! The writer stores Y800 (raw 8-bit luma), MJPEG or PNG frames with an
//! `idx1` index. The reader streams video chunks sequentially, so it also
//! replays recordings whose final index was never written (power loss) and
//! OpenDML (`AVIX`) continuations. It decodes Y800/GREY, 8/24/32-bit DIB,
//! MJPEG and PNG frames to 8-bit luma.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
};

/// How video frames are stored on disk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoFormat {
    /// Motion JPEG: typically 10–20× smaller than raw, visually lossless at
    /// quality ≥ 90, plays in VLC/FFmpeg and replays natively in Rekkefly.
    #[default]
    Mjpeg,
    /// Lossless PNG frames: bit-exact, usually 1.5–3× smaller than raw.
    Png,
    /// Uncompressed 8-bit luma (Y800): bit-exact, no CPU, largest files.
    Raw,
}
impl VideoFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Mjpeg => "MJPEG (small)",
            Self::Png => "PNG (lossless)",
            Self::Raw => "Raw Y800 (lossless, large)",
        }
    }
    fn fourcc(self) -> [u8; 4] {
        match self {
            Self::Mjpeg => *b"MJPG",
            Self::Png => *b"MPNG",
            Self::Raw => *b"Y800",
        }
    }
}

/// Encode one 8-bit luma image for an AVI chunk. `Raw` copies the pixels.
/// `compact` builds per-frame Huffman tables: about 7% smaller MJPEG for
/// about 15% more encoding time, for offline clips rather than live recording.
pub fn encode(
    format: VideoFormat,
    quality: u8,
    compact: bool,
    width: u32,
    height: u32,
    pixels: &[u8],
    out: &mut Vec<u8>,
) -> Result<()> {
    out.clear();
    ensure!(
        pixels.len() == width as usize * height as usize,
        "Frame length does not match dimensions"
    );
    match format {
        VideoFormat::Raw => out.extend_from_slice(pixels),
        VideoFormat::Mjpeg => {
            ensure!(
                width <= u16::MAX as u32 && height <= u16::MAX as u32,
                "Frame too large for JPEG"
            );
            let mut encoder = jpeg_encoder::Encoder::new(&mut *out, quality.clamp(1, 100));
            encoder.set_optimized_huffman_tables(compact);
            encoder
                .encode(
                    pixels,
                    width as u16,
                    height as u16,
                    jpeg_encoder::ColorType::Luma,
                )
                .context("Encode JPEG frame")?;
        }
        VideoFormat::Png => {
            use image::ImageEncoder;
            image::codecs::png::PngEncoder::new_with_quality(
                &mut *out,
                image::codecs::png::CompressionType::Fast,
                image::codecs::png::FilterType::Sub,
            )
            .write_image(pixels, width, height, image::ExtendedColorType::L8)
            .context("Encode PNG frame")?;
        }
    }
    Ok(())
}

fn u32le(out: &mut impl Write, value: u32) -> Result<()> {
    out.write_all(&value.to_le_bytes())?;
    Ok(())
}
fn u16le(out: &mut impl Write, value: u16) -> Result<()> {
    out.write_all(&value.to_le_bytes())?;
    Ok(())
}
fn start_chunk(out: &mut (impl Write + Seek), tag: &[u8; 4]) -> Result<u64> {
    out.write_all(tag)?;
    let p = out.stream_position()?;
    u32le(out, 0)?;
    Ok(p)
}
fn patch(out: &mut (impl Write + Seek), position: u64, value: u32) -> Result<()> {
    let end = out.stream_position()?;
    out.seek(SeekFrom::Start(position))?;
    u32le(out, value)?;
    out.seek(SeekFrom::Start(end))?;
    Ok(())
}
fn end_chunk(out: &mut (impl Write + Seek), position: u64) -> Result<()> {
    let size = out.stream_position()? - position - 4;
    ensure!(size <= u32::MAX as u64, "AVI chunk exceeds 4 GiB");
    patch(out, position, size as u32)
}
pub(crate) fn create_new(path: &Path) -> Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("Create {}", path.display()))
}

/// RIFF AVI with one intra-only video stream and an idx1 index. Callers keep
/// files below the classic 2 GiB AVI limit by starting new segments.
pub struct AviWriter {
    out: BufWriter<File>,
    width: u32,
    height: u32,
    rate: u32,
    scale: u32,
    total_frames: u64,
    stream_frames: u64,
    avih_buffer: u64,
    strh_buffer: u64,
    avih_rate: u64,
    largest: u32,
    movi_size: u64,
    movi_base: u64,
    position: u64,
    index: Vec<(u32, u32)>,
    finished: bool,
}
impl AviWriter {
    /// `rate / scale` is the nominal frame rate (for example 30000/1001).
    pub fn create(
        path: &Path,
        width: u32,
        height: u32,
        rate: u32,
        scale: u32,
        format: VideoFormat,
    ) -> Result<Self> {
        ensure!(rate > 0 && scale > 0, "AVI frame rate must be positive");
        ensure!(
            width <= u16::MAX as u32 && height <= u16::MAX as u32,
            "Frame too large for AVI"
        );
        let image_bytes = width * height;
        let micros = ((1_000_000u64 * scale as u64) / rate as u64).clamp(1, u32::MAX as u64) as u32;
        let mut out = BufWriter::with_capacity(1024 * 1024, create_new(path)?);
        out.write_all(b"RIFF")?;
        u32le(&mut out, 0)?;
        out.write_all(b"AVI ")?;
        let hdrl = start_chunk(&mut out, b"LIST")?;
        out.write_all(b"hdrl")?;
        out.write_all(b"avih")?;
        u32le(&mut out, 56)?;
        u32le(&mut out, micros)?;
        let avih_rate = out.stream_position()?;
        u32le(&mut out, image_bytes.saturating_mul(rate / scale.max(1)))?;
        u32le(&mut out, 0)?;
        u32le(&mut out, 0x10)?;
        let total_frames = out.stream_position()?;
        u32le(&mut out, 0)?;
        u32le(&mut out, 0)?;
        u32le(&mut out, 1)?;
        let avih_buffer = out.stream_position()?;
        u32le(&mut out, image_bytes)?;
        u32le(&mut out, width)?;
        u32le(&mut out, height)?;
        for _ in 0..4 {
            u32le(&mut out, 0)?;
        }
        let strl = start_chunk(&mut out, b"LIST")?;
        out.write_all(b"strl")?;
        out.write_all(b"strh")?;
        u32le(&mut out, 56)?;
        out.write_all(b"vids")?;
        out.write_all(&format.fourcc())?;
        u32le(&mut out, 0)?;
        u16le(&mut out, 0)?;
        u16le(&mut out, 0)?;
        u32le(&mut out, 0)?;
        u32le(&mut out, scale)?;
        u32le(&mut out, rate)?;
        u32le(&mut out, 0)?;
        let stream_frames = out.stream_position()?;
        u32le(&mut out, 0)?;
        let strh_buffer = out.stream_position()?;
        u32le(&mut out, image_bytes)?;
        u32le(&mut out, u32::MAX)?;
        u32le(&mut out, 0)?;
        u16le(&mut out, 0)?;
        u16le(&mut out, 0)?;
        u16le(&mut out, width as u16)?;
        u16le(&mut out, height as u16)?;
        out.write_all(b"strf")?;
        u32le(&mut out, 40)?;
        u32le(&mut out, 40)?;
        u32le(&mut out, width)?;
        u32le(&mut out, height)?;
        u16le(&mut out, 1)?;
        u16le(&mut out, if format == VideoFormat::Raw { 8 } else { 24 })?;
        u32le(&mut out, u32::from_le_bytes(format.fourcc()))?;
        u32le(
            &mut out,
            if format == VideoFormat::Raw {
                image_bytes
            } else {
                image_bytes * 3
            },
        )?;
        for value in [0, 0, 0, 0] {
            u32le(&mut out, value)?;
        }
        end_chunk(&mut out, strl)?;
        end_chunk(&mut out, hdrl)?;
        let movi_size = start_chunk(&mut out, b"LIST")?;
        let movi_base = out.stream_position()?;
        out.write_all(b"movi")?;
        let position = out.stream_position()?;
        Ok(Self {
            out,
            width,
            height,
            rate,
            scale,
            total_frames,
            stream_frames,
            avih_buffer,
            strh_buffer,
            avih_rate,
            largest: 0,
            movi_size,
            movi_base,
            position,
            index: Vec::new(),
            finished: false,
        })
    }
    pub fn frames(&self) -> u32 {
        self.index.len() as u32
    }
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    /// File size if a chunk of `bytes` were appended and the file finished.
    pub fn size_after(&self, bytes: usize) -> u64 {
        self.position
            + 8
            + bytes as u64
            + (bytes % 2) as u64
            + 8
            + (self.index.len() as u64 + 1) * 16
    }
    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        ensure!(!self.finished, "AVI segment already finished");
        ensure!(data.len() <= u32::MAX as usize / 2, "AVI frame too large");
        let offset = (self.position - self.movi_base) as u32;
        let size = data.len() as u32;
        self.out.write_all(b"00dc")?;
        u32le(&mut self.out, size)?;
        // One contiguous write: no per-row copies or palette conversion.
        self.out.write_all(data)?;
        if size % 2 == 1 {
            self.out.write_all(&[0])?;
        }
        self.index.push((offset, size));
        self.largest = self.largest.max(size);
        self.position += 8 + size as u64 + (size % 2) as u64;
        Ok(())
    }
    pub fn finish(&mut self) -> Result<()> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        let frames = self.frames();
        end_chunk(&mut self.out, self.movi_size)?;
        self.out.write_all(b"idx1")?;
        u32le(&mut self.out, frames * 16)?;
        for &(offset, size) in &self.index {
            self.out.write_all(b"00dc")?;
            u32le(&mut self.out, 0x10)?;
            u32le(&mut self.out, offset)?;
            u32le(&mut self.out, size)?;
        }
        patch(&mut self.out, self.total_frames, frames)?;
        patch(&mut self.out, self.stream_frames, frames)?;
        if self.largest > 0 {
            patch(&mut self.out, self.avih_buffer, self.largest)?;
            patch(&mut self.out, self.strh_buffer, self.largest)?;
            let per_second = self.largest as u64 * self.rate as u64 / self.scale as u64;
            patch(
                &mut self.out,
                self.avih_rate,
                per_second.min(u32::MAX as u64) as u32,
            )?;
        }
        end_chunk(&mut self.out, 4)?;
        self.out.flush()?;
        self.out.get_ref().sync_all().ok();
        Ok(())
    }
}
impl Drop for AviWriter {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Codec {
    Gray,
    Dib8,
    Dib24,
    Dib32,
    Jpeg,
    Png,
}
/// Stream properties from the AVI headers.
#[derive(Clone, Debug, PartialEq)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,
    /// Nominal frames per second from the stream header.
    pub fps: f64,
    pub rate: u32,
    pub scale: u32,
    /// Frame count from the headers; zero for an unfinished recording.
    pub frames: u64,
    pub fourcc: String,
}
pub struct AviReader<R> {
    input: R,
    info: VideoInfo,
    codec: Codec,
    bottom_up: bool,
    palette: [u8; 256],
    stream: [u8; 2],
    movi: Vec<(u64, u64)>,
    segment: usize,
    position: u64,
    /// Body offset and size of the last stored frame, for zero-length
    /// "repeat previous frame" chunks (read again only when needed).
    last: Option<(u64, u64)>,
}
fn read_u32(r: &mut impl Read) -> std::io::Result<u32> {
    let mut b = [0; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}
fn tag(r: &mut impl Read) -> std::io::Result<[u8; 4]> {
    let mut b = [0; 4];
    r.read_exact(&mut b)?;
    Ok(b)
}
fn le32(b: &[u8], at: usize) -> u32 {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
        .unwrap_or(0)
}
fn le16(b: &[u8], at: usize) -> u16 {
    b.get(at..at + 2)
        .map(|s| u16::from_le_bytes(s.try_into().unwrap()))
        .unwrap_or(0)
}
impl AviReader<BufReader<File>> {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("Open video {}", path.display()))?;
        Self::new(BufReader::with_capacity(1 << 20, file))
            .with_context(|| format!("Read AVI {}", path.display()))
    }
}
impl<R: Read + Seek> AviReader<R> {
    pub fn new(mut input: R) -> Result<Self> {
        let length = input.seek(SeekFrom::End(0))?;
        input.seek(SeekFrom::Start(0))?;
        ensure!(
            tag(&mut input)? == *b"RIFF",
            "Not an AVI file (no RIFF header)"
        );
        let size = read_u32(&mut input)? as u64;
        ensure!(tag(&mut input)? == *b"AVI ", "Not an AVI file");
        // An unfinished recording has a zero size: read to the end instead.
        let riff_end = if size == 0 || 8 + size > length {
            length
        } else {
            8 + size
        };
        let mut avih = Vec::new();
        let mut strh = Vec::new();
        let mut strf = Vec::new();
        let mut stream = None;
        let mut movi = Vec::new();
        let mut position = 12;
        let mut stream_index = 0u32;
        while position + 8 <= riff_end {
            input.seek(SeekFrom::Start(position))?;
            let id = tag(&mut input)?;
            let size = read_u32(&mut input)? as u64;
            let data = position + 8;
            let end = if id == *b"LIST" && (size == 0 || data + size > riff_end) {
                riff_end
            } else {
                (data + size).min(riff_end)
            };
            if id == *b"LIST" && (size >= 4 || size == 0) {
                let kind = tag(&mut input)?;
                if kind == *b"hdrl" {
                    let mut p = data + 4;
                    while p + 8 <= end {
                        input.seek(SeekFrom::Start(p))?;
                        let id = tag(&mut input)?;
                        let size = read_u32(&mut input)? as u64;
                        let body = p + 8;
                        if id == *b"avih" {
                            avih = vec![0; size.min(256) as usize];
                            input.read_exact(&mut avih)?;
                        } else if id == *b"LIST" && tag(&mut input)? == *b"strl" {
                            let mut q = body + 4;
                            let mut header = Vec::new();
                            let mut format = Vec::new();
                            while q + 8 <= (body + size).min(end) {
                                input.seek(SeekFrom::Start(q))?;
                                let id = tag(&mut input)?;
                                let n = read_u32(&mut input)? as u64;
                                if id == *b"strh" {
                                    header = vec![0; n.min(256) as usize];
                                    input.read_exact(&mut header)?;
                                } else if id == *b"strf" {
                                    format = vec![0; n.min(40 + 1024) as usize];
                                    input.read_exact(&mut format)?;
                                }
                                q += 8 + n + n % 2;
                            }
                            if stream.is_none() && header.get(0..4) == Some(b"vids") {
                                stream = Some(stream_index);
                                strh = header;
                                strf = format;
                            }
                            stream_index += 1;
                        }
                        p = body + size + size % 2;
                    }
                } else if kind == *b"movi" {
                    movi.push((data + 4, end));
                }
            }
            position = data + size + size % 2;
            if id == *b"LIST" && end == riff_end && size == 0 {
                break;
            }
        }
        // OpenDML continuation segments: RIFF 'AVIX' → LIST 'movi'.
        let mut position = riff_end + riff_end % 2;
        while position + 24 <= length {
            input.seek(SeekFrom::Start(position))?;
            if tag(&mut input)? != *b"RIFF" {
                break;
            }
            let size = read_u32(&mut input)? as u64;
            let end = if size == 0 {
                length
            } else {
                (position + 8 + size).min(length)
            };
            if tag(&mut input)? == *b"AVIX" && tag(&mut input)? == *b"LIST" {
                let n = read_u32(&mut input)? as u64;
                if tag(&mut input)? == *b"movi" {
                    let start = position + 24;
                    movi.push((
                        start,
                        if n == 0 {
                            end
                        } else {
                            (start - 4 + n).min(end)
                        },
                    ));
                }
            }
            position = end + end % 2;
        }
        let stream = stream.context("AVI has no video stream")?;
        ensure!(!movi.is_empty(), "AVI has no frame data (movi list)");
        ensure!(strf.len() >= 20, "AVI video format header is missing");
        let width = le32(&strf, 4);
        let raw_height = le32(&strf, 8) as i32;
        let height = raw_height.unsigned_abs();
        let bits = le16(&strf, 14);
        let compression = le32(&strf, 16).to_le_bytes();
        let handler = strh.get(4..8).map(|s| [s[0], s[1], s[2], s[3]]);
        ensure!(
            (1..=16384).contains(&width) && (1..=16384).contains(&height),
            "Unsupported AVI dimensions {width}×{height}"
        );
        let upper = compression.map(|c| c.to_ascii_uppercase());
        let codec = match &upper {
            [0, 0, 0, 0] | [3, 0, 0, 0] => match bits {
                8 => Codec::Dib8,
                24 => Codec::Dib24,
                32 => Codec::Dib32,
                _ => bail!("Unsupported uncompressed AVI bit depth {bits}"),
            },
            b"Y800" | b"Y8  " | b"GREY" | b"GRAY" | [b'Y', b'8', 0, 0] => Codec::Gray,
            b"MJPG" | b"JPEG" | b"AVRN" | b"DMB1" => Codec::Jpeg,
            b"MPNG" | b"PNG1" | b"PNG " => Codec::Png,
            _ => bail!(
                "Unsupported AVI codec {:?}; install FFmpeg to read it",
                String::from_utf8_lossy(&compression)
            ),
        };
        let mut palette = [0u8; 256];
        for (i, value) in palette.iter_mut().enumerate() {
            *value = i as u8;
        }
        if codec == Codec::Dib8 {
            let header = le32(&strf, 0) as usize;
            let colors = match le32(&strf, 32) as usize {
                0 => 256,
                n => n.min(256),
            };
            for (i, value) in palette.iter_mut().take(colors).enumerate() {
                let at = header + i * 4;
                if let Some(c) = strf.get(at..at + 3) {
                    *value = ((c[2] as u32 * 77 + c[1] as u32 * 150 + c[0] as u32 * 29) >> 8) as u8;
                }
            }
        }
        let scale = le32(&strh, 20);
        let rate = le32(&strh, 24);
        let micros = le32(&avih, 0);
        let (rate, scale) = if rate > 0 && scale > 0 {
            (rate, scale)
        } else if micros > 0 {
            (1_000_000, micros)
        } else {
            (30, 1)
        };
        let frames = match le32(&strh, 32) {
            0 => le32(&avih, 16),
            n => n,
        } as u64;
        let info = VideoInfo {
            width,
            height,
            fps: rate as f64 / scale as f64,
            rate,
            scale,
            frames,
            fourcc: String::from_utf8_lossy(&handler.unwrap_or(compression))
                .trim_matches(char::from(0))
                .to_string(),
        };
        let first = movi[0].0;
        Ok(Self {
            input,
            info,
            codec,
            bottom_up: raw_height > 0,
            palette,
            stream: [b'0' + (stream / 10) as u8 % 10, b'0' + (stream % 10) as u8],
            movi,
            segment: 0,
            position: first,
            last: None,
        })
    }
    pub fn info(&self) -> &VideoInfo {
        &self.info
    }
    /// Return to the first frame.
    pub fn rewind(&mut self) {
        self.segment = 0;
        self.position = self.movi[0].0;
        self.last = None;
    }
    /// Read the next stored video chunk. `Ok(false)` marks the end of the
    /// file, including a cleanly truncated final chunk.
    pub fn next_chunk(&mut self, data: &mut Vec<u8>, decode: bool) -> Result<bool> {
        loop {
            let Some(&(_, end)) = self.movi.get(self.segment) else {
                return Ok(false);
            };
            if self.position + 8 > end {
                self.segment += 1;
                if let Some(&(start, _)) = self.movi.get(self.segment) {
                    self.position = start;
                }
                continue;
            }
            self.input.seek(SeekFrom::Start(self.position))?;
            let id = tag(&mut self.input)?;
            let size = read_u32(&mut self.input)? as u64;
            if id == *b"LIST" {
                // 'rec ' groups: descend into their children.
                self.position += 12;
                continue;
            }
            let body = self.position + 8;
            if body + size > end {
                // Truncated final chunk after an interrupted write.
                self.position = end;
                continue;
            }
            self.position = body + size + size % 2;
            let video = id[0..2] == self.stream && (id[2..] == *b"dc" || id[2..] == *b"db");
            if !video {
                continue;
            }
            // A zero-length chunk repeats the previous frame. It counts as a
            // frame both when playing and when skipping, once a frame exists.
            let (offset, length) = if size == 0 {
                match self.last {
                    Some(previous) => previous,
                    None => continue,
                }
            } else {
                self.last = Some((body, size));
                (body, size)
            };
            if decode {
                self.input.seek(SeekFrom::Start(offset))?;
                data.resize(length as usize, 0);
                self.input.read_exact(data)?;
            }
            return Ok(true);
        }
    }
    /// Decode a chunk from `next_chunk` to top-down 8-bit luma.
    pub fn decode(&self, data: &[u8], out: &mut Vec<u8>) -> Result<()> {
        let (w, h) = (self.info.width as usize, self.info.height as usize);
        out.clear();
        match self.codec {
            Codec::Gray => {
                ensure!(data.len() >= w * h, "Short grayscale frame");
                out.extend_from_slice(&data[..w * h]);
            }
            Codec::Dib8 | Codec::Dib24 | Codec::Dib32 => {
                let bytes = match self.codec {
                    Codec::Dib8 => 1,
                    Codec::Dib24 => 3,
                    _ => 4,
                };
                let stride = (w * bytes).div_ceil(4) * 4;
                ensure!(data.len() >= stride * (h - 1) + w * bytes, "Short frame");
                out.reserve(w * h);
                for y in 0..h {
                    let row = if self.bottom_up { h - 1 - y } else { y };
                    let line = &data[row * stride..row * stride + w * bytes];
                    if bytes == 1 {
                        out.extend(line.iter().map(|&i| self.palette[i as usize]));
                    } else {
                        out.extend(line.chunks_exact(bytes).map(|p| {
                            ((p[2] as u32 * 77 + p[1] as u32 * 150 + p[0] as u32 * 29) >> 8) as u8
                        }));
                    }
                }
            }
            Codec::Jpeg => {
                let options = zune_core::options::DecoderOptions::default()
                    .jpeg_set_out_colorspace(zune_core::colorspace::ColorSpace::Luma);
                let mut decoder =
                    zune_jpeg::JpegDecoder::new_with_options(std::io::Cursor::new(data), options);
                let pixels = decoder.decode().context("Decode JPEG frame")?;
                let (dw, dh) = decoder.dimensions().unwrap_or((0, 0));
                ensure!(
                    dw == w && dh == h && pixels.len() == w * h,
                    "JPEG frame size {dw}×{dh} differs from the AVI header"
                );
                *out = pixels;
            }
            Codec::Png => {
                let image = image::load_from_memory_with_format(data, image::ImageFormat::Png)
                    .context("Decode PNG frame")?
                    .into_luma8();
                ensure!(
                    image.width() as usize == w && image.height() as usize == h,
                    "PNG frame size differs from the AVI header"
                );
                *out = image.into_raw();
            }
        }
        Ok(())
    }
    /// Skip `count` frames without reading or decoding them.
    pub fn skip(&mut self, count: u64) -> Result<u64> {
        let mut skipped = 0;
        let mut scratch = Vec::new();
        while skipped < count && self.next_chunk(&mut scratch, false)? {
            skipped += 1;
        }
        Ok(skipped)
    }
    /// Read and decode the next frame. `Ok(false)` at the end of the file.
    pub fn next_frame(&mut self, chunk: &mut Vec<u8>, pixels: &mut Vec<u8>) -> Result<bool> {
        if !self.next_chunk(chunk, true)? {
            return Ok(false);
        }
        self.decode(chunk, pixels)?;
        Ok(true)
    }
}

/// True if a file starts with a RIFF AVI header.
pub fn is_avi(path: &Path) -> bool {
    let mut header = [0; 12];
    File::open(path)
        .and_then(|mut f| f.read_exact(&mut header))
        .is_ok()
        && &header[0..4] == b"RIFF"
        && &header[8..12] == b"AVI "
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pattern(w: u32, h: u32, k: u32) -> Vec<u8> {
        (0..w * h).map(|i| ((i * 7 + k * 13) % 251) as u8).collect()
    }
    #[test]
    fn raw_avi_layout_preserves_pixels_padding_and_index() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.avi");
        let pixels = pattern(33, 33, 0);
        let mut writer = AviWriter::create(&path, 33, 33, 120, 1, VideoFormat::Raw).unwrap();
        writer.write(&pixels).unwrap();
        writer.write(&pixels).unwrap();
        writer.finish().unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8,
            bytes.len()
        );
        let movi = bytes.windows(4).position(|b| b == b"movi").unwrap();
        assert_eq!(&bytes[movi + 4..movi + 8], b"00dc");
        assert_eq!(&bytes[movi + 12..movi + 12 + pixels.len()], &pixels);
        assert_eq!(bytes[movi + 12 + pixels.len()], 0);
        let second = movi + 12 + pixels.len() + 1;
        assert_eq!(&bytes[second..second + 4], b"00dc");
        let strh = bytes.windows(4).position(|b| b == b"strh").unwrap();
        assert_eq!(&bytes[strh + 8..strh + 16], b"vidsY800");
        let idx = bytes.windows(4).position(|b| b == b"idx1").unwrap();
        assert_eq!(
            u32::from_le_bytes(bytes[idx + 16..idx + 20].try_into().unwrap()),
            4
        );
        assert_eq!(u32::from_le_bytes(bytes[48..52].try_into().unwrap()), 2);
    }
    #[test]
    fn every_format_round_trips_through_the_native_reader() {
        let dir = tempfile::tempdir().unwrap();
        for format in [VideoFormat::Raw, VideoFormat::Png, VideoFormat::Mjpeg] {
            let path = dir.path().join(format!("{format:?}.avi"));
            let mut writer = AviWriter::create(&path, 64, 48, 30000, 1001, format).unwrap();
            let mut chunk = Vec::new();
            let frames: Vec<Vec<u8>> = (0..5)
                .map(|k| {
                    (0..64 * 48)
                        .map(|i| (60 + (i % 64) * 2 + k * 3) as u8)
                        .collect()
                })
                .collect();
            for f in &frames {
                encode(format, 95, false, 64, 48, f, &mut chunk).unwrap();
                writer.write(&chunk).unwrap();
            }
            writer.finish().unwrap();
            let mut reader = AviReader::open(&path).unwrap();
            assert_eq!(reader.info().width, 64);
            assert_eq!(reader.info().height, 48);
            assert_eq!(reader.info().frames, 5);
            assert!((reader.info().fps - 29.97).abs() < 0.01);
            let mut pixels = Vec::new();
            for expected in &frames {
                assert!(reader.next_frame(&mut chunk, &mut pixels).unwrap());
                let worst = pixels
                    .iter()
                    .zip(expected)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                if format == VideoFormat::Mjpeg {
                    assert!(worst <= 6, "{format:?} error {worst}");
                } else {
                    assert_eq!(&pixels, expected, "{format:?} must be lossless");
                }
            }
            assert!(!reader.next_frame(&mut chunk, &mut pixels).unwrap());
            reader.rewind();
            assert_eq!(reader.skip(3).unwrap(), 3);
            assert!(reader.next_frame(&mut chunk, &mut pixels).unwrap());
        }
    }
    #[test]
    fn unfinished_recording_without_index_is_still_readable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash.avi");
        let mut writer = AviWriter::create(&path, 32, 32, 60, 1, VideoFormat::Raw).unwrap();
        for k in 0..3 {
            writer.write(&pattern(32, 32, k)).unwrap();
        }
        writer.out.flush().unwrap();
        // Simulate power loss: no finish(), sizes stay zero, final chunk cut.
        let mut bytes = std::fs::read(&path).unwrap();
        writer.finished = true;
        drop(writer);
        bytes.truncate(bytes.len() - 100);
        std::fs::write(&path, &bytes).unwrap();
        let mut reader = AviReader::open(&path).unwrap();
        let (mut chunk, mut pixels) = (Vec::new(), Vec::new());
        let mut count = 0;
        while reader.next_frame(&mut chunk, &mut pixels).unwrap() {
            assert_eq!(pixels, pattern(32, 32, count));
            count += 1;
        }
        assert_eq!(count, 2);
    }
    #[test]
    fn zero_length_repeat_chunks_count_the_same_when_playing_and_skipping() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dropped.avi");
        let mut writer = AviWriter::create(&path, 8, 8, 10, 1, VideoFormat::Raw).unwrap();
        // Frames: A, repeat, repeat, B, repeat.
        writer.write(&[10; 64]).unwrap();
        writer.write(&[]).unwrap();
        writer.write(&[]).unwrap();
        writer.write(&[20; 64]).unwrap();
        writer.write(&[]).unwrap();
        writer.finish().unwrap();
        let mut reader = AviReader::open(&path).unwrap();
        let (mut c, mut p) = (Vec::new(), Vec::new());
        let mut played = Vec::new();
        while reader.next_frame(&mut c, &mut p).unwrap() {
            played.push(p[0]);
        }
        assert_eq!(played, [10, 10, 10, 20, 20]);
        for start in 0..5 {
            reader.rewind();
            assert_eq!(reader.skip(start).unwrap(), start);
            assert!(reader.next_frame(&mut c, &mut p).unwrap());
            assert_eq!(p[0], played[start as usize], "start {start}");
        }
    }
    #[test]
    fn bottom_up_palette_dib_is_converted_to_top_down_luma() {
        // Hand-built 8-bit DIB AVI, as written by many lab tools.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dib.avi");
        let (w, h) = (3u32, 2u32);
        let mut strf = Vec::new();
        for v in [40, w, h] {
            strf.extend_from_slice(&v.to_le_bytes());
        }
        strf.extend_from_slice(&1u16.to_le_bytes());
        strf.extend_from_slice(&8u16.to_le_bytes());
        for v in [0u32, 8, 0, 0, 256, 0] {
            strf.extend_from_slice(&v.to_le_bytes());
        }
        for i in 0..256u32 {
            let g = 255 - i as u8; // Inverted palette.
            strf.extend_from_slice(&[g, g, g, 0]);
        }
        let mut strh = vec![0u8; 56];
        strh[0..4].copy_from_slice(b"vids");
        strh[20..24].copy_from_slice(&1u32.to_le_bytes());
        strh[24..28].copy_from_slice(&25u32.to_le_bytes());
        let chunk = |id: &[u8], body: &[u8]| {
            let mut c = id.to_vec();
            c.extend_from_slice(&(body.len() as u32).to_le_bytes());
            c.extend_from_slice(body);
            if body.len() % 2 == 1 {
                c.push(0);
            }
            c
        };
        let list = |kind: &[u8], body: Vec<u8>| {
            let mut b = kind.to_vec();
            b.extend(body);
            chunk(b"LIST", &b)
        };
        let strl = list(
            b"strl",
            [chunk(b"strh", &strh), chunk(b"strf", &strf)].concat(),
        );
        let hdrl = list(b"hdrl", [chunk(b"avih", &[0; 56]), strl].concat());
        // Bottom row first, rows padded to four bytes.
        let frame = [3u8, 4, 5, 0, 0, 1, 2, 0];
        let movi = list(b"movi", chunk(b"00db", &frame));
        let mut riff = b"AVI ".to_vec();
        riff.extend([hdrl, movi].concat());
        std::fs::write(&path, chunk(b"RIFF", &riff)).unwrap();
        let mut reader = AviReader::open(&path).unwrap();
        assert_eq!(reader.info().fps, 25.0);
        let (mut c, mut p) = (Vec::new(), Vec::new());
        assert!(reader.next_frame(&mut c, &mut p).unwrap());
        assert_eq!(p, vec![255, 254, 253, 252, 251, 250]);
    }
}
