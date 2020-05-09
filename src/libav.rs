use ffmpeg4_ffi::sys::{
    avcodec_find_decoder, avformat_alloc_context, avformat_open_input, AVCodec,
    AVCodecID_AV_CODEC_ID_H264 as AV_CODEC_ID_H264, AVFormatContext, avformat_find_stream_info, av_dump_format,
};
use std::path::Path;

pub unsafe fn count_video_frames(path: &str) {
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

    av_dump_format(
        avctx,
        0,
        input_path_cstr.as_ptr(),
        0,
    );
}
