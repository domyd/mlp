use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
pub mod parser;

fn main() -> std::io::Result<()> {
    let file = File::open(r"C:\Users\domin\Desktop\mkv-sync-testing\frozen2\55.thd")?;
    let mut reader = BufReader::new(file);

    access_unit(&mut reader);

    // let mut file_header = [0u8; 8];
    // reader.read_exact(&mut file_header)?;

    // if (!(file_header[4] == 0xF8
    //     && file_header[5] == 0x72
    //     && file_header[6] == 0x6F
    //     && file_header[7] == 0xBA))
    // {
    //     panic!("invalid TrueHD file header");
    // }

    // access_unit(&mut reader);

    Ok(())
}

fn access_unit<T: Seek + Read>(reader: &mut T) {
    // nibble: v(4)
    // length: u(12)
    // timing: u(16)
    let mut x = [0u8; 4];
    // let mut
    reader.read_exact(&mut x);

    let mut length: u16 = 0b0000_0000_0000_0000;
    length &= ((x[0] << 4) as u16) << 4;
    length |= x[1] as u16;

    dbg!(length);

    // println!("{:#b}", length);
    // println!("{:#b}", x[0]);
    // println!("{:#b}", x[1]);
    // println!("{:#b}", x[2]);
    // println!("{:#b}", x[3]);
    println!("0b{:08b}", x[0]);
    println!("0b{:08b}", x[1]);
    println!("0b{:08b}", x[2]);
    println!("0b{:08b}", x[3]);
}
