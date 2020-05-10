use ffmpeg4_ffi::sys::{
    av_packet_alloc, av_packet_free, av_read_frame, avcodec_alloc_context3,
    avcodec_find_decoder, avcodec_free_context, avcodec_open2, avcodec_parameters_to_context, avformat_close_input, avformat_find_stream_info, avformat_open_input,
    AVCodec, AVCodecContext, AVCodecParameters,
    AVFormatContext, AVMediaType_AVMEDIA_TYPE_VIDEO as AVMEDIA_TYPE_VIDEO, AVPacket,
};

pub unsafe fn count_video_frames(path: &str) -> Option<u32> {
    let input_path_cstr = std::ffi::CString::new(path).expect("to c str");

    let mut avctx: *mut AVFormatContext = std::ptr::null_mut();
    assert_eq!(
        avformat_open_input(
            &mut avctx,
            input_path_cstr.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ),
        0
    );

    assert!(avformat_find_stream_info(avctx, std::ptr::null_mut()) >= 0);

    let av_streams = std::slice::from_raw_parts((*avctx).streams, (*avctx).nb_streams as usize);
    let mut av_stream = av_streams
            .iter()
            .enumerate()
            .filter_map(|(i, &stream)| {
                let codec_params: *mut AVCodecParameters = (*stream).codecpar;
                assert!(!codec_params.is_null());
                let codec: *mut AVCodec = avcodec_find_decoder((*codec_params).codec_id);
                assert!(!codec.is_null());

                if (*codec_params).codec_type == AVMEDIA_TYPE_VIDEO {
                    let video_stream_index: i32 = i as i32;
                    Some((video_stream_index, codec, codec_params))
                } else {
                    None
                }
            })
            .take(1);

    if let Some((video_stream_index, codec, codec_params)) = av_stream.next() {
        // do the thing
        let mut codec_ctx: *mut AVCodecContext = avcodec_alloc_context3(codec);
        assert!(!codec_ctx.is_null());
        assert_eq!(avcodec_parameters_to_context(codec_ctx, codec_params), 0);
        assert_eq!(avcodec_open2(codec_ctx, codec, std::ptr::null_mut()), 0);

        let mut packet: *mut AVPacket = av_packet_alloc();
        assert!(!packet.is_null());

        // do the thing
        let mut i: u32 = 0;
        while av_read_frame(avctx, packet) >= 0 {
            if (*packet).stream_index == video_stream_index {
                i += 1;
            }
        }

        // cleanup
        av_packet_free(&mut packet);
        avcodec_free_context(&mut codec_ctx);
        avformat_close_input(&mut avctx);

        return Some(i);
    }

    None
}
