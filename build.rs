use fs_extra;
use std::fs::{read_dir, remove_dir_all};
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.lock");

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_path = PathBuf::from(&out_dir);

    let mut header_files: Vec<PathBuf> = vec![];
    let incl_path: PathBuf = [&manifest_dir, "external", "ffmpeg", "include"]
        .iter()
        .collect();
    visit_dirs(incl_path.as_path(), &mut header_files).unwrap();

    for relative_header_path in header_files
        .iter()
        .flat_map(|h| h.strip_prefix(&manifest_dir))
        .flat_map(|p| p.to_str())
    {
        println!("cargo:rerun-if-changed={}", relative_header_path);
    }

    setup(&out_path, &PathBuf::from(&manifest_dir));
}

#[cfg(target_os = "windows")]
fn setup(out_path: &PathBuf, root_dir: &PathBuf) {
    let bin_dir = out_path.as_path().join("bin");
    let lib_dir = out_path.as_path().join("lib");
    let ffmpeg_bundle = root_dir
        .join("external")
        .join("ffmpeg")
        .join("ffmpeg-4.2.2-win64.zip");

    if !is_target_state(&out_path) {
        let bundle = extract(&ffmpeg_bundle);

        let bin_src_dir = bundle.as_path().join("bin");
        let lib_src_dir = bundle.as_path().join("lib");

        copy_dir(&bin_src_dir, &bin_dir);
        copy_dir(&lib_src_dir, &lib_dir);

        assert!(is_target_state(&out_path));
        remove_dir_all(&bundle).unwrap();
    }

    println!(
        "cargo:rustc-link-search=native={}",
        (lib_dir.to_str().unwrap())
    );
    println!(
        "cargo:rustc-link-search=native={}",
        (bin_dir.to_str().unwrap())
    );
}

#[cfg(target_os = "macos")]
fn setup(out_path: &PathBuf, root_dir: &PathBuf) {
    let bin_dir = out_path.as_path().join("bin");
    let ffmpeg_bundle = root_dir
        .join("external")
        .join("ffmpeg")
        .join("ffmpeg-4.2.2-macos64.zip");

    if !is_target_state(&out_path) {
        let bundle = extract(&ffmpeg_bundle);
        let bin_src_dir = bundle.as_path().join("bin");

        copy_dir(&bin_src_dir, &bin_dir);

        assert!(is_target_state(&out_path));
        remove_dir_all(&bundle).unwrap();
    }

    let dylibs = vec![
        "libavcodec.58.dylib",
        "libavdevice.58.dylib",
        "libavfilter.7.dylib",
        "libavformat.58.dylib",
        "libavutil.56.dylib",
        "libswresample.3.dylib",
        "libswscale.5.dylib",
    ];

    for lib in dylibs.iter() {
        // create symlink that doesn't have the version, so ld can find it
        let dylib_path = {
            let mut p = bin_dir.clone();
            p.push(lib);
            p
        };
        let symlink_path = {
            let lib_symlink = format!("{}.dylib", lib.split('.').collect::<Vec<&str>>()[0]);
            let mut p = bin_dir.clone();
            p.push(lib_symlink);
            p
        };
        if symlink_path.exists() {
            std::fs::remove_file(&symlink_path).unwrap();
        }
        std::os::unix::fs::symlink(&dylib_path, &symlink_path).unwrap();

        // we don't need to emit these because the ffmpeg4-ffi crate does that for us already
        //println!("cargo:rustc-link-lib=dylib={}", lib);
    }

    println!(
        "cargo:rustc-link-search=native={}",
        (bin_dir.to_str().unwrap())
    );
}

fn copy_dir(src: &PathBuf, dst: &PathBuf) {
    let cp_opts = {
        let mut o = fs_extra::dir::CopyOptions::new();
        o.copy_inside = true;
        o
    };
    if !dst.exists() {
        let mut dst_cp = dst.clone();
        dst_cp.pop();
        fs_extra::dir::copy(&src, &dst_cp, &cp_opts).unwrap();
    }
}

fn visit_dirs(dir: &Path, entries: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if dir.is_dir() {
        for entry in read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                visit_dirs(&path, entries)?;
            }
            entries.push(path);
        }
    }
    Ok(())
}

fn extract(zip_path: &PathBuf) -> PathBuf {
    // `tar` is available on Windows since 1803
    Command::new("tar")
        .args(&["-xf", zip_path.to_str().unwrap()])
        .current_dir(zip_path.parent().unwrap())
        .output()
        .expect("failed to extract archive");

    let mut path = zip_path.clone();
    path.set_extension("");
    path
}

fn is_target_state(path: &Path) -> bool {
    if cfg!(target_os = "macos") {
        path.join("bin").exists()
    } else if cfg!(target_os = "windows") {
        path.join("bin").exists() && path.join("lib").exists()
    } else {
        panic!("OS currently not supported.")
    }
}
