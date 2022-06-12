use super::{
    dsp, truehd, AVCodecType, AVError, AVFormatContext, AVFrame, AVPacket, AVStream,
    DecodedThdFrame, DemuxErr, Framerate, MediaDuration, ThdDecodePacket, ThdFrameHeader,
    ThdOverrun, ThdSegment, VideoMetadata,
};
use crate::Segment;
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, info, trace, warn};
use std::{
    fmt::Display,
    io::{Seek, SeekFrom, Write}, path::Path,
};
use truehd::ThdMetadata;

pub struct SegmentDemuxStats {
    pub video_frames: u32,
    pub video_metadata: VideoMetadata,
    pub thd_frames: u32,
    pub thd_frames_original: u32,
    pub thd_metadata: ThdMetadata,
}

#[derive(Debug, Copy, Clone)]
pub struct DemuxOptions {
    pub thd_stream_id: Option<i32>,
}

impl Default for DemuxOptions {
    fn default() -> Self {
        DemuxOptions {
            thd_stream_id: None,
        }
    }
}

impl SegmentDemuxStats {
    pub fn audio_duration(&self) -> f64 {
        self.thd_metadata.duration(self.thd_frames)
    }

    pub fn video_duration(&self) -> f64 {
        self.video_metadata.duration(self.video_frames)
    }

    pub fn audio_overrun(&self) -> f64 {
        self.audio_duration() - self.video_duration()
    }
}

pub struct DemuxStats {
    pub segments: Vec<SegmentDemuxStats>,
}

impl DemuxStats {
    pub fn overrun(&self) -> ThdOverrun {
        ThdOverrun {
            acc: {
                let (v, a) = self.duration();
                a - v
            },
        }
    }

    pub fn video_metadata(&self) -> Option<VideoMetadata> {
        self.segments.first().map(|x| x.video_metadata)
    }

    pub fn thd_metadata(&self) -> Option<ThdMetadata> {
        self.segments.first().map(|x| x.thd_metadata)
    }

    pub fn duration(&self) -> (f64, f64) {
        let (f_video, f_audio): (u32, u32) = self
            .segments
            .iter()
            .map(|t| (t.video_frames, t.thd_frames))
            .fold((0u32, 0u32), |(v_acc, a_acc), (v, a)| {
                ((v_acc + v), (a_acc + a))
            });

        self.segments.first().map_or((0f64, 0f64), |s| {
            let (meta_video, meta_audio) = (s.video_metadata, s.thd_metadata);
            (meta_video.duration(f_video), meta_audio.duration(f_audio))
        })
    }
}

// returns the number of frames to cut off the end
fn adjust_gap(tail: &ThdDecodePacket, head: &ThdDecodePacket, overrun: &ThdOverrun) -> u32 {
    let head_mono = &head.mono;
    let tail_mono = &tail.mono;

    let head_samples: Vec<i32> = head_mono.samples.iter().map(|f| f.value).collect();
    let tail_samples: Vec<i32> = tail_mono.samples.iter().map(|f| f.value).collect();

    let silence_threshold = 100i32;
    let head_max = head_samples.iter().map(|n| n.abs()).max().unwrap_or(0);
    let tail_max = tail_samples.iter().map(|n| n.abs()).max().unwrap_or(0);

    let covariance = dsp::covariance(&head_samples, &tail_samples);
    debug!("Frame covariance is {:.7}", covariance);

    let adjust = if head_max < silence_threshold && tail_max < silence_threshold {
        // We're dealing with a silent section here, which means the covariance
        // value isn't going to be very significant. So we'll fall back to
        // minimizing desync based on the overrun accumulator.
        debug!(
            "Both frames at segment boundary appear to only contain silent audio. Falling back to overrun correction. (head max: {}, tail max: {})", 
            head_max,
            tail_max
        );

        if overrun.samples() >= 20 {
            info!("Deleted silent frame to correct for audio sync drift.");
            1
        } else {
            0
        }
    } else {
        if covariance > 0.95 {
            info!("Deleted frame with duplicate audio content.");
            1
        } else {
            debug!("No duplicate audio content at segment boundary.");
            0
        }
    };

    if adjust == 0 {
        info!("Segment boundary is OK, no adjustment necessary.");
    }

    adjust
}

#[derive(Debug, Clone)]
pub struct ThdStreamInfo {
    pub index: i32,
    pub id: i32,
    pub language: Option<String>,
    pub metadata: ThdMetadata,
}

impl Display for ThdStreamInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Stream: index: {}, id: {:#X}, language: {}, {}",
            self.index,
            self.id,
            self.language
                .as_ref()
                .map(|s| s.as_ref())
                .unwrap_or("unknown"),
            self.metadata,
        )
    }
}

pub fn thd_streams<'a, P: AsRef<Path>>(path: P) -> Result<Vec<ThdStreamInfo>, AVError> {
    let mut avctx = AVFormatContext::open(&path)?;
    let streams = avctx.streams()?;
    let thd_streams: Vec<ThdStreamInfo> = streams
        .iter()
        .filter_map(|s| {
            if s.codec.id == ffmpeg4_ffi::sys::AVCodecID_AV_CODEC_ID_TRUEHD {
                let metadata = get_thd_metadata(s);
                Some(ThdStreamInfo {
                    index: s.stream.index,
                    id: s.stream.id,
                    language: None,
                    metadata,
                })
            } else {
                None
            }
        })
        .collect();

    Ok(thd_streams)
}

pub fn demux_thd<W: Write + Seek>(
    segments: &[Segment],
    options: &DemuxOptions,
    mut out_writer: W,
) -> Result<DemuxStats, AVError> {
    let mut stats: DemuxStats = DemuxStats {
        segments: Vec::with_capacity(segments.len()),
    };
    let mut previous_segment: Option<ThdSegment> = None;

    debug!("Using demux options: {:?}", options);

    let file_count = segments.len();
    for (i, segment) in segments.iter().enumerate() {
        info!(
            "Processing file {}/{} ('{}') ...",
            i + 1,
            file_count,
            segment.path.display()
        );

        // check overrun and apply sync, if necessary
        if let Some(ref mut prev) = previous_segment {
            info!("Checking segment file gap.");

            // match audio data
            // `tail` is the last TrueHD frame of the previous segment
            // `head` is the first TrueHD frame of the current segment
            let (tail, tail_header) = { prev.last_group_of_frames.last().unwrap() };
            let head = {
                // open the current segment and decode only the first TrueHD frame
                let mut avctx = AVFormatContext::open(&segment.path)?;
                let streams = avctx.streams()?;
                let decoded_head_frame = streams
                    .iter()
                    .find(|&s| s.codec.id == ffmpeg4_ffi::sys::AVCodecID_AV_CODEC_ID_TRUEHD)
                    .and_then(|thd_stream| decode_head_frame(&mut avctx, thd_stream).ok()?);
                match decoded_head_frame {
                    Some(decoded_frame) => decoded_frame,
                    None => {
                        warn!(
                            "No TrueHD frames found in {}. This segment will be skipped.",
                            segment.path.display()
                        );
                        continue;
                    }
                }
            };

            trace!("tail: {}", tail.original);
            trace!("head: {}", head.original);

            trace!("tail MONO: {}", tail.mono);
            trace!("head MONO: {}", head.mono);

            let overrun = stats.overrun();
            debug!(
                "Uncorrected overrun would be {} samples.",
                overrun.samples()
            );

            let n_delete = adjust_gap(&tail, &head, &overrun);
            if n_delete > 0 {
                // delete the most recently written frame by moving the file
                // cursor back
                &out_writer
                    .seek(SeekFrom::Current(-(tail_header.length as i64)))
                    .unwrap();
                let mut prev_stats = stats.segments.last_mut().unwrap();
                prev_stats.thd_frames -= 1;
            }
        }

        debug!("Overrun is now {} samples.", stats.overrun().samples());
        debug!("Copying TrueHD stream to output ...");
        let mut avctx = AVFormatContext::open(&segment.path)?;
        let streams = avctx.streams()?;

        let video_stream = streams
            .iter()
            .find(|&s| s.codec_type() == AVCodecType::Video)
            .ok_or(DemuxErr::NoVideoStreamFound)?;
        let thd_stream = streams
            .iter()
            .find(|&s| {
                s.codec.id == ffmpeg4_ffi::sys::AVCodecID_AV_CODEC_ID_TRUEHD
                    && options.thd_stream_id.map_or(true, |i| s.stream.id == i)
            })
            .ok_or(DemuxErr::NoTrueHdStreamFound)?;

        let segment = write_thd_segment(
            &segment,
            &mut avctx,
            video_stream,
            thd_stream,
            &mut out_writer,
        )?;

        let segment_overrun = ThdOverrun {
            acc: segment.overrun(),
        };
        debug!("Segment overrun is {} samples.", segment_overrun.samples());
        stats.segments.push(SegmentDemuxStats {
            video_frames: segment.num_video_frames,
            thd_frames_original: segment.num_frames,
            thd_frames: segment.num_frames,
            thd_metadata: segment.thd_metadata,
            video_metadata: segment.video_metadata,
        });

        previous_segment = Some(segment);
    }

    debug!("Overrun is now {} samples.", stats.overrun().samples());
    info!("Done!");

    return Ok(stats);
}

fn write_thd_segment<W: Write + Seek>(
    segment: &Segment,
    format_context: &mut AVFormatContext,
    video_stream: &AVStream,
    thd_stream: &AVStream,
    mut thd_writer: W,
) -> Result<ThdSegment, AVError> {
    let (video_metadata, thd_metadata) = (
        get_video_metadata(video_stream),
        get_thd_metadata(thd_stream),
    );

    // set up progress bar
    let start_time = thd_stream.stream.start_time;
    let duration = thd_stream.stream.duration as u64;
    let progress = ProgressBar::new(duration);
    progress.set_draw_delta(duration / 100);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{wide_bar}] {eta}")
            .progress_chars("#>-"),
    );

    debug!("Video: {:?}, Audio: {:?}", video_metadata, thd_metadata);

    let (mut num_frames, mut num_video_frames) = (0u32, 0u32);

    // keeps the packets of the most recent group of frames
    // (all frames "belonging" to one major sync)
    let mut packet_queue: Vec<AVPacket> = Vec::with_capacity(128);

    // keeps track of the frame headers of the last group of frames we've written
    let mut frame_queue: Vec<ThdFrameHeader> = Vec::with_capacity(128);

    // keeps track of the progress, for UI purposes
    let mut prev_pts: i64 = start_time;

    while let Ok(packet) = format_context.read_frame() {
        if packet.of_stream(video_stream) {
            // increase video frame counter (which we need in order to calculate
            // the precise video duration)
            num_video_frames += 1;
        } else if packet.of_stream(thd_stream) {
            // update progress
            // `set_position` doesn't seem to respect the
            // draw_delta we set for performance reasons
            let pts = packet.pkt.pts;
            let progress_delta = (pts - prev_pts).max(0);
            progress.inc(progress_delta as u64);
            prev_pts = pts;

            // copy the TrueHD frame to the output
            let pkt_slice = packet.as_slice();
            &thd_writer.write_all(&pkt_slice)?;

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

    progress.finish_and_clear();

    trace!(
        "Last group of frames is {} frames long.",
        packet_queue.len()
    );
    debug!(
        "{} TrueHD frames have been written to the output.",
        num_frames
    );
    debug!("Encountered {} video frames.", num_video_frames);

    // ffmpeg sometimes has an issue with identifying the very first HEVC frame
    // of a stream, which leads to a wrong frame count. So we cross-check that
    // count against what the MPLS file says we _should_ have, and take the
    // corrected count for calculating the overrun.
    let corrected_video_frames = if let Some(n) = segment.video_frames {
        if n as u32 != num_video_frames {
            warn!("Counted {} frames, but expected {}. Using the expected number for calculating overrun.", 
            num_video_frames, n);
        }
        n as u32
    } else {
        num_video_frames
    };

    let decoded_frames = truehd::decode(thd_stream, packet_queue)?
        .into_iter()
        .zip(frame_queue.into_iter())
        .map(|(d, h)| (d, h))
        .collect();

    Ok(ThdSegment {
        last_group_of_frames: decoded_frames,
        num_frames,
        num_video_frames: corrected_video_frames,
        video_metadata,
        thd_metadata,
    })
}

// returns the very last decoded TrueHD frame of the given file and stream
pub fn decode_tail_frame(
    format_context: &mut AVFormatContext,
    stream: &AVStream,
) -> Result<Option<ThdDecodePacket>, AVError> {
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

    let mut decoded_frames = truehd::decode(stream, packets)?;
    Ok(decoded_frames.pop())
}

// returns the very first decoded TrueHD frame of the given file and stream
pub fn decode_head_frame(
    format_context: &mut AVFormatContext,
    stream: &AVStream,
) -> Result<Option<ThdDecodePacket>, AVError> {
    let mut av_frame = AVFrame::new();

    let mut a_ctx = stream.get_codec_context()?;
    a_ctx.open(&stream)?;

    while let Ok(packet) = format_context.read_frame() {
        if !packet.of_stream(stream) {
            continue;
        }

        a_ctx.decode_frame(&packet, &mut av_frame)?;
        let decoded_frame = DecodedThdFrame::from(&av_frame);
        let mono_frame = truehd::downmix_mono(&av_frame, &a_ctx)?;
        let decoded_mono_frame = DecodedThdFrame::from(&mono_frame);

        return Ok(Some(ThdDecodePacket {
            original: decoded_frame,
            mono: decoded_mono_frame,
        }));
    }

    return Ok(None);
}

fn get_video_metadata(video_stream: &AVStream) -> VideoMetadata {
    let frame_rate = video_stream.stream.r_frame_rate;
    VideoMetadata {
        framerate: Framerate {
            numerator: frame_rate.num,
            denominator: frame_rate.den,
        },
    }
}

pub fn get_thd_metadata(thd_stream: &AVStream) -> ThdMetadata {
    let sample_rate = thd_stream.codec_params.sample_rate as u32;
    let frame_size = (sample_rate / 1200) as u8;
    let channels = thd_stream.codec_params.channels as u8;
    ThdMetadata {
        sample_rate,
        frame_size,
        channels,
    }
}
