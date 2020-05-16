use fs_extra;
use std::fs::{read_dir, remove_dir_all, remove_file, File};
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.lock");

    let ffmpeg_version = "4.2.2";
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

    let bin_dir = out_path.as_path().join("bin");
    let lib_dir = out_path.as_path().join("lib");

    if !is_target_state(&out_path) {
        let ff_dev_dl = download_ffmpeg(&out_dir, "dev", ffmpeg_version);
        let ff_dev = extract_7z(&ff_dev_dl);

        let ff_shared_dl = download_ffmpeg(&out_dir, "shared", ffmpeg_version);
        let ff_shared = extract_7z(&ff_shared_dl);

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

        let bin_src_dir = ff_shared.as_path().join("bin");
        let lib_src_dir = ff_dev.as_path().join("lib");

        copy_dir(&bin_src_dir, &bin_dir);
        copy_dir(&lib_src_dir, &lib_dir);

        assert!(is_target_state(&out_path));

        remove_dir_all(&ff_dev).unwrap();
        remove_dir_all(&ff_shared).unwrap();
        remove_file(&ff_dev_dl).unwrap();
        remove_file(&ff_shared_dl).unwrap();
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

fn is_target_state(path: &Path) -> bool {
    path.join("bin").exists() && path.join("lib").exists()
}

fn download_ffmpeg(out_dir: &str, build_type: &str, version: &str) -> PathBuf {
    let url = reqwest::Url::parse(&format!(
        "https://ffmpeg.zeranoe.com/builds/win64/{t}/ffmpeg-{v}-win64-{t}-lgpl.zip",
        t = build_type,
        v = version
    ))
    .unwrap();

    let dest_path = {
        let fname = url
            .path_segments()
            .and_then(|segments| segments.last())
            .and_then(|name| if name.is_empty() { None } else { Some(name) })
            .unwrap();

        let mut path = PathBuf::from(out_dir);
        path.push(fname);
        path
    };

    if !dest_path.exists() {
        let mut response = reqwest::blocking::get(url).unwrap();
        let mut file = File::create(dest_path.as_path()).unwrap();
        response.copy_to(&mut file).unwrap();
    }

    dest_path
}

fn extract_7z(path: &PathBuf) -> PathBuf {
    Command::new("7z")
        .args(&["x", path.to_str().unwrap()])
        .current_dir({
            let mut p = path.clone();
            p.pop();
            p
        })
        .output()
        .expect("failed to extract archive");

    let mut path = path.clone();
    path.set_extension("");
    path
}
