use super::{
    truehd, AVCodecType, AVError, AVFormatContext, AVFrame, AVPacket, AVStream, DecodedThdFrame,
    DemuxErr, OtherErr, ThdFrameHeader, ThdOverrun, ThdSegment,
};
use log::{debug, info, trace, warn};
use std::{
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

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
                    .find(|&s| s.codec_id() == ffmpeg4_ffi::sys::AVCodecID_AV_CODEC_ID_TRUEHD)
                    .unwrap();
                match decode_head_frame(&avctx, thd_stream)? {
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
            .find(|&s| s.codec_id() == ffmpeg4_ffi::sys::AVCodecID_AV_CODEC_ID_TRUEHD)
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
    }

    debug!("Overrun is now {} samples.", overrun_acc.samples());
    info!("Done!");

    return Ok(overrun_acc);
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

    let decoded_frames = truehd::decode(audio_stream, packet_queue)?
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

// returns the very last decoded TrueHD frame of the given file and stream
pub fn decode_tail_frame(
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

    let mut decoded_frames = truehd::decode(stream, packets)?;
    Ok(decoded_frames.pop())
}

// returns the very first decoded TrueHD frame of the given file and stream
pub fn decode_head_frame(
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
