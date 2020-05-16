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
