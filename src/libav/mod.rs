use ::log::*;
use ffmpeg4_ffi::sys as ff;
use std::{
    convert::TryInto,
    io::{Error, Seek, SeekFrom, Write},
    ops::{Add, AddAssign},
    path::{Path, PathBuf},
};

pub mod av_codec_context;
pub mod av_format_context;
pub mod av_frame;
pub mod av_packet;
pub use av_codec_context::AVCodecContext;
pub use av_format_context::{AVCodecType, AVFormatContext, AVStream};
pub use av_frame::AVFrame;
pub use av_packet::{AVPacket, AVPacketReader};

#[derive(Debug)]
pub enum AVError {
    IoErr(std::io::Error),
    FFMpegErr(i32),
    DemuxErr(DemuxErr),
    OtherErr(OtherErr),
}

#[derive(Debug)]
pub enum DemuxErr {
    NoVideoStreamFound,
    NoTrueHdStreamFound,
    NoTrueHdFramesEncountered,
}

#[derive(Debug)]
pub enum OtherErr {
    FilePathIsNotUtf8(PathBuf),
}

impl From<DemuxErr> for AVError {
    fn from(err: DemuxErr) -> Self {
        AVError::DemuxErr(err)
    }
}

impl From<OtherErr> for AVError {
    fn from(err: OtherErr) -> Self {
        AVError::OtherErr(err)
    }
}

impl From<Error> for AVError {
    fn from(err: Error) -> Self {
        AVError::IoErr(err)
    }
}

impl AVError {
    pub fn log(&self) {
        match self {
            AVError::IoErr(e) => {
                error!("{}", e.to_string());
            }
            AVError::FFMpegErr(i) => {
                error!("ffmpeg error code: {}", i);
            }
            AVError::DemuxErr(e) => {
                let msg = match e {
                    DemuxErr::NoTrueHdStreamFound => "No TrueHD stream found.",
                    DemuxErr::NoVideoStreamFound => "No video stream found.",
                    DemuxErr::NoTrueHdFramesEncountered => "No TrueHD frames encountered.",
                };
                error!("{}", msg);
            }
            AVError::OtherErr(e) => {
                let msg = match e {
                    OtherErr::FilePathIsNotUtf8(path) => {
                        format!("File path is not valid UTF-8: {}", path.to_string_lossy())
                    }
                };
                error!("{}", msg);
            }
        }
    }
}

/// A very light-weight header that only contains a length and a flag of whether
/// the encoded frame contains a major sync header.
pub struct ThdFrameHeader {
    pub length: usize,
    pub has_major_sync: bool,
}

impl ThdFrameHeader {
    pub fn from_bytes(bytes: &[u8]) -> Option<ThdFrameHeader> {
        if bytes.len() < 8 {
            return None;
        }

        let access_unit_length = {
            let bytes = [(bytes[0] & 0x0f), bytes[1]];
            (u16::from_be_bytes(bytes) as usize) * 2
        };
        let has_major_sync = {
            let major_sync_flag = u32::from_be_bytes(bytes[4..8].try_into().unwrap());
            major_sync_flag == 0xf8726fba
        };

        return Some(ThdFrameHeader {
            length: access_unit_length,
            has_major_sync,
        });
    }
}

pub mod log {
    use ffmpeg4_ffi::sys as ff;
    use log::{log, Level};
    use std::{
        ffi::{c_void, CStr},
        os::raw::c_char,
    };

    pub fn configure_rust_log(level: i32) {
        unsafe {
            ff::av_log_set_level(level);
            ff::av_log_set_callback(Some(ffmpeg_log_adapter));
        }
    }

    extern "C" fn ffmpeg_log_adapter(
        ptr: *mut c_void,
        level: i32,
        fmt: *const c_char,
        vl: *mut ff::__va_list_tag,
    ) {
        if let Some(log_level) = match level {
            0 | 8 | 16 => Some(Level::Error),
            24 => Some(Level::Warn),
            32 => Some(Level::Info),
            40 => Some(Level::Debug),
            48 => Some(Level::Debug),
            56 => Some(Level::Trace),
            _ => None,
        } {
            // ffmpeg puts the formatted log line into this buffer
            let mut buf = [c_char::MIN; 1024];
            let mut print_prefix: i32 = 1;
            let _ret = unsafe {
                ff::av_log_format_line(
                    ptr,
                    level,
                    fmt,
                    vl,
                    buf.as_mut_ptr(),
                    buf.len() as i32,
                    &mut print_prefix as *mut i32,
                )
            };

            let message = unsafe { CStr::from_ptr(buf.as_ptr()).to_string_lossy() };

            // strip the newline character off the message, if it has one
            // also, if it doesn't have any chars, there's nothing to print
            if let Some(message_without_newline) = match message.chars().last() {
                Some(c) if c == '\n' => Some(&message[..message.len() - 1]),
                Some(_) => Some(&message[..]),
                None => None,
            } {
                log!(target: "ffmpeg", log_level, "ffmpeg: {}", message_without_newline);
            }
        }
    }
}

pub struct ThdSample {
    // TODO: half the size of it by packing the 24 bit data with the 8 bit channel number
    // most significant byte (native endianness) contains the channel number as a u8,
    // least three significant bytes contain the 24-bit audio sample
    // packed: u32
    value: i32,
    channel: u8,
}

impl ThdSample {
    pub fn from_bytes(bytes: [u8; 4], channel: u8) -> Self {
        let sample = i32::from_ne_bytes(bytes) / 256;
        ThdSample::new(sample, channel)
    }

    pub fn new(sample: i32, channel: u8) -> Self {
        ThdSample {
            value: sample,
            channel,
        }
    }
}

pub struct DecodedThdFrame {
    samples: Vec<ThdSample>,
    num_channels: u8,
}

impl DecodedThdFrame {
    pub fn new(bytes: &[u8], bytes_per_sample: usize, channels: u8) -> DecodedThdFrame {
        let samples: Vec<ThdSample> = bytes
            .chunks_exact(bytes_per_sample)
            .enumerate()
            .map(|(i, c)| {
                ThdSample::from_bytes(c.try_into().unwrap(), (i % channels as usize) as u8)
            })
            .collect();
        DecodedThdFrame {
            samples,
            num_channels: channels,
        }
    }

    pub fn get_max_distance(&self, other: &Self) -> i32 {
        let mut max_distance: i32 = 0;
        for (l, r) in self.samples.iter().zip(other.samples.iter()) {
            let diff = (l.value - r.value).abs();
            max_distance = diff.max(max_distance);
        }
        max_distance
    }

    pub fn matches_approximately(&self, other: &Self, tolerance: i32) -> bool {
        // TODO: this is a very naive algorithm, do better
        for (l, r) in self.samples.iter().zip(other.samples.iter()) {
            let diff = (l.value - r.value).abs();
            if diff > tolerance {
                return false;
            }
        }
        return true;
    }

    pub fn is_silence(&self, threshold: i32) -> bool {
        self.samples.iter().all(|s| s.value.abs() < threshold)
    }
}

impl From<&AVFrame> for DecodedThdFrame {
    fn from(frame: &AVFrame) -> Self {
        let bytes = frame.as_slice();
        let bytes_per_sample = frame.bytes_per_sample();
        let channels = frame.channels();
        DecodedThdFrame::new(bytes, bytes_per_sample, channels)
    }
}

fn decode_thd(stream: &AVStream, packets: Vec<AVPacket>) -> Result<Vec<DecodedThdFrame>, AVError> {
    if stream.codec_id() != ff::AVCodecID_AV_CODEC_ID_TRUEHD {
        panic!("attempted to decode a non-TrueHD stream.");
    }

    if packets.is_empty() {
        return Ok(Vec::new());
    }

    // allocate an AVFrame which will be filled with the decoded audio frame
    let mut av_frame = AVFrame::new();

    let mut a_ctx = stream.get_codec_context()?;
    a_ctx.open(&stream)?;

    let mut frame_buf: Vec<DecodedThdFrame> = Vec::with_capacity(packets.len());
    for packet in packets {
        a_ctx.decode_frame(&packet, &mut av_frame)?;
        let decoded_frame = DecodedThdFrame::from(&av_frame);
        frame_buf.push(decoded_frame);
    }

    Ok(frame_buf)
}

fn print_frames(frames: &Vec<DecodedThdFrame>) {
    for sample in frames.iter().flat_map(|f| f.samples.iter()) {
        println!("ch[{}], s[{}]", sample.channel, sample.value)
    }
}

// returns the very last decoded TrueHD frame of the given file and stream
fn read_thd_audio_tail(
    format_context: &AVFormatContext,
    stream: &AVStream,
) -> Result<Option<DecodedThdFrame>, AVError> {
    let mut packets: Vec<AVPacket> = Vec::with_capacity(128);

    while let Ok(packet) = format_context.read_frame() {
        if !packet.of_stream(stream) {
            continue;
        }

        let thd_frame = ThdFrameHeader::from_bytes(&packet.as_slice()).unwrap();
        if thd_frame.length != packet.data_len() {
            panic!("TrueHD frame header indicates a length of {} bytes, but AVPacket's data size is {} bytes", thd_frame.length, packet.data_len());
        }
        if thd_frame.has_major_sync {
            // clear the frame queue, new major sync is in town
            packets.truncate(0);
        }

        packets.push(packet);
    }

    let mut decoded_frames = decode_thd(stream, packets)?;
    Ok(decoded_frames.pop())
}

// returns the very first decoded TrueHD frame of the given file and stream
fn read_thd_audio_head(
    format_context: &AVFormatContext,
    stream: &AVStream,
) -> Result<Option<DecodedThdFrame>, AVError> {
    let mut av_frame = AVFrame::new();

    let mut a_ctx = stream.get_codec_context()?;
    a_ctx.open(&stream)?;

    while let Ok(packet) = format_context.read_frame() {
        if !packet.of_stream(stream) {
            continue;
        }

        a_ctx.decode_frame(&packet, &mut av_frame)?;
        let decoded_frame = DecodedThdFrame::from(&av_frame);
        return Ok(Some(decoded_frame));
    }

    return Ok(None);
}

pub fn thd_audio_read_test(file: &PathBuf, head: bool, threshold: i32) -> Result<(), AVError> {
    let file_path = file
        .to_str()
        .ok_or(OtherErr::FilePathIsNotUtf8(PathBuf::from(file)))?;
    let avctx = AVFormatContext::new(&file_path)?;
    let streams = avctx.get_streams()?;

    info!("processing file '{}' ...", file_path);

    let audio_stream = streams
        .iter()
        .find(|&s| s.codec_id() == ff::AVCodecID_AV_CODEC_ID_TRUEHD)
        .ok_or(DemuxErr::NoTrueHdStreamFound)?;

    if head {
        match read_thd_audio_head(&avctx, &audio_stream)? {
            Some(frame) => {
                println!("is silence: {}", &frame.is_silence(threshold));
                print_frames(&vec![frame]);
            }
            None => {
                println!("no frames encountered");
            }
        }
    } else {
        match read_thd_audio_tail(&avctx, &audio_stream)? {
            Some(frame) => {
                println!("is silence: {}", &frame.is_silence(threshold));
                print_frames(&vec![frame]);
            }
            None => {
                println!("no frames encountered");
            }
        }
    }

    Ok(())
}

pub struct DemuxOptions {
    pub audio_match_threshold: i32,
}

pub fn demux_thd<W: Write + Seek>(
    files: &[&Path],
    options: &DemuxOptions,
    out_writer: &mut W,
) -> Result<ThdOverrun, AVError> {
    let mut overrun_acc = ThdOverrun { acc: 0.0 };
    let mut previous_segment: Option<ThdSegment> = None;

    let file_count = files.len();
    for (i, &file) in files.iter().enumerate() {
        let file_path = file
            .to_str()
            .ok_or(OtherErr::FilePathIsNotUtf8(PathBuf::from(file)))?;
        info!(
            "Processing file {}/{} ('{}') ...",
            i + 1,
            file_count,
            file_path
        );

        // check overrun and apply sync, if necessary
        if let Some(ref mut prev) = previous_segment {
            info!("Checking segment file gap.",);

            // match audio data
            // `tail` is the last TrueHD frame of the previous segment
            // `head` is the first TrueHD frame of the current segment
            let (tail, tail_header) = { prev.last_group_of_frames.last().unwrap() };
            let head = {
                // open the current segment and decode only the first TrueHD frame
                let avctx = AVFormatContext::new(&file_path)?;
                let streams = avctx.get_streams()?;
                let thd_stream = streams
                    .iter()
                    .find(|&s| s.codec_id() == ff::AVCodecID_AV_CODEC_ID_TRUEHD)
                    .unwrap();
                match read_thd_audio_head(&avctx, thd_stream)? {
                    Some(decoded_frame) => decoded_frame,
                    None => {
                        warn!(
                            "No TrueHD frames found in {}. This segment will be skipped.",
                            file_path
                        );
                        continue;
                    }
                }
            };

            debug!(
                "Uncorrected overrun would be {} samples.",
                overrun_acc.samples()
            );

            // if only one of them is silent, they almost certainly don't
            // contain duplicated audio.
            if head.is_silence(options.audio_match_threshold)
                && tail.is_silence(options.audio_match_threshold)
            {
                // fallback to bresenham's algorithm, since we can't reliably
                // match two silent audio frame against each other
                debug!(
                    "Both frames at segment boundary appear to only contain silent audio. Falling back to overrun correction."
                );
                if overrun_acc.samples() >= 20 {
                    if let Some((_, header)) = prev.last_group_of_frames.last() {
                        // delete the most recently written frame by moving the file
                        // cursor back
                        out_writer
                            .seek(SeekFrom::Current(-(header.length as i64)))
                            .unwrap();
                        overrun_acc.sub_frames(1);
                        info!("Deleted silent frame to correct for audio sync drift.");
                    }
                } else {
                    info!("Segment boundary is OK.");
                }
            } else {
                // do audio matching
                debug!("Comparing audio content ...");

                let max_distance = head.get_max_distance(tail);
                debug!("Largest sample value difference: {}", max_distance);

                // warn the user if two audio frames contain suspiciously similar
                // audio which escaped the threshold
                if max_distance > options.audio_match_threshold
                    && max_distance < options.audio_match_threshold * 5
                {
                    warn!(
                        "Audio content of these two frames is suspiciously similar, yet escapes your set threshold of {}. The maximum sample distance between these frames was {}. Consider manually verifying that these frames contain duplicate audio and perhaps adjust the threshold parameter.", 
                        options.audio_match_threshold,
                        max_distance);
                }

                if head.matches_approximately(tail, options.audio_match_threshold) {
                    // delete the most recently written frame by moving the file
                    // cursor back
                    out_writer
                        .seek(SeekFrom::Current(-(tail_header.length as i64)))
                        .unwrap();
                    overrun_acc.sub_frames(1);
                    info!("Deleted frame with duplicate audio content.");
                } else {
                    info!("Segment boundary is OK.");
                    debug!("No duplicate audio content found at segment boundary.");
                }
            }
        }

        debug!("Overrun is now {} samples.", overrun_acc.samples());
        debug!("Copying TrueHD stream to output ...");
        let avctx = AVFormatContext::new(file_path)?;
        let streams = avctx.get_streams()?;

        let video_stream = streams
            .iter()
            .find(|&s| s.codec_type() == AVCodecType::Video)
            .ok_or(DemuxErr::NoVideoStreamFound)?;
        let thd_stream = streams
            .iter()
            .find(|&s| s.codec_id() == ff::AVCodecID_AV_CODEC_ID_TRUEHD)
            .ok_or(DemuxErr::NoTrueHdStreamFound)?;

        previous_segment = Some(write_thd_segment(
            &avctx,
            video_stream,
            thd_stream,
            out_writer,
        )?);

        // adjust overrun
        if let Some(ref s) = previous_segment {
            let segment_overrun = ThdOverrun {
                acc: s.audio_overrun(),
            };
            debug!("Segment overrun is {} samples.", segment_overrun.samples());
            overrun_acc += segment_overrun.acc;
        }
        overrun_acc += previous_segment
            .as_ref()
            .map(|s| s.audio_overrun())
            .unwrap_or(0.0);
    }

    debug!("Overrun is now {} samples.", overrun_acc.samples());
    info!("Done!");

    return Ok(overrun_acc);
}

struct ThdSegment {
    last_group_of_frames: Vec<(DecodedThdFrame, ThdFrameHeader)>,
    num_frames: u32,
    num_video_frames: u32,
}

impl ThdSegment {
    pub fn audio_duration(&self) -> f64 {
        // we're assuming 48 KHz here, which is usually
        // true but doesn't have to be
        // TODO
        (self.num_frames * 40) as f64 / 48000_f64
    }

    pub fn video_duration(&self) -> f64 {
        // we're assuming 23.976 fps here, which isn't always true
        // TODO
        (self.num_video_frames * 1001) as f64 / 24000_f64
    }

    pub fn audio_overrun(&self) -> f64 {
        self.audio_duration() - self.video_duration()
    }
}

fn write_thd_segment<W: Write + Seek>(
    format_context: &AVFormatContext,
    video_stream: &AVStream,
    audio_stream: &AVStream,
    thd_writer: &mut W,
) -> Result<ThdSegment, AVError> {
    let (mut num_frames, mut num_video_frames) = (0u32, 0u32);

    // keeps the packets of the most recent group of frames
    // (all frames "belonging" to one major sync)
    let mut packet_queue: Vec<AVPacket> = Vec::with_capacity(128);

    // keeps track of the frame headers of the last group of frames we've written
    let mut frame_queue: Vec<ThdFrameHeader> = Vec::with_capacity(128);

    while let Ok(packet) = format_context.read_frame() {
        if packet.of_stream(video_stream) {
            // increase video frame counter (which we need in order to calculate
            // the precise video duration)
            num_video_frames += 1;
        } else if packet.of_stream(audio_stream) {
            // copy the TrueHD frame to the output
            let pkt_slice = packet.as_slice();
            thd_writer.write_all(&pkt_slice)?;

            // push frame header to queue (we want to remember the last
            // n frame headers we saw)
            let frame = ThdFrameHeader::from_bytes(&pkt_slice).unwrap();
            if frame.has_major_sync {
                packet_queue.truncate(0);
                frame_queue.truncate(0);
            }
            packet_queue.push(packet);
            frame_queue.push(frame);

            // increase THD frame counter
            num_frames += 1;
        }
    }

    debug!("Encountered {} video frames.", num_video_frames);
    debug!(
        "{} TrueHD frames have been written to the output.",
        num_frames
    );
    trace!(
        "Last group of frames is {} frames long.",
        packet_queue.len()
    );

    let decoded_frames = decode_thd(audio_stream, packet_queue)?
        .into_iter()
        .zip(frame_queue.into_iter())
        .map(|(d, h)| (d, h))
        .collect();

    Ok(ThdSegment {
        last_group_of_frames: decoded_frames,
        num_frames,
        num_video_frames,
    })
}

#[derive(Debug, Copy, Clone)]
pub struct ThdOverrun {
    pub acc: f64,
}

impl ThdOverrun {
    pub fn sub_frames(&mut self, frames: i32) {
        self.acc = self.acc - ((frames * 40) as f64 / 48000_f64);
    }

    pub fn samples(&self) -> i32 {
        (self.acc * 48000_f64).round() as i32
    }
}

impl Add for ThdOverrun {
    type Output = ThdOverrun;

    fn add(self, rhs: ThdOverrun) -> Self::Output {
        ThdOverrun {
            acc: self.acc + rhs.acc,
        }
    }
}

impl Add<f64> for ThdOverrun {
    type Output = ThdOverrun;
    fn add(self, rhs: f64) -> Self::Output {
        ThdOverrun {
            acc: self.acc + rhs,
        }
    }
}

impl AddAssign<f64> for ThdOverrun {
    fn add_assign(&mut self, rhs: f64) {
        self.acc += rhs;
    }
}
