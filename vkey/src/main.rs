// MIT/Apache2 License

use std::{
    env,
    fs::File,
    io::{prelude::*, BufReader, BufWriter},
};

fn main() {
    let mut args = env::args_os().skip(1);
    let mut infile = BufReader::new(File::open(args.next().unwrap()).unwrap());
    let mut outfile = BufWriter::new(File::create(args.next().unwrap()).unwrap());

    let mut buffer = String::new();
    infile.read_to_string(&mut buffer).unwrap();

    let data: Vec<Vec<serde_json::Value>> = serde_json::from_str(&buffer).unwrap();

    // begin writing to the outfile
    outfile
        .write_all(
            b"// Generated by yaww-vkey, licensed under MIT/Apache2 License

use gluten_keyboard::Key;
use winapi::{ctypes::c_int, um::winuser};

#[inline]
pub fn convert_vkey_to_key(vk: c_int, extended: bool) -> Option<Key> {
    match (vk, extended) {",
        )
        .unwrap();

    // read in data and write it to the file
    data.iter().for_each(|d| {
        if let (Some(name), Some(extended), Some(val)) = (
            d.get(0).and_then(|d| d.as_str()),
            d.get(1).and_then(|d| d.as_bool()),
            d.get(2).and_then(|d| d.as_str()),
        ) {
            let mut name = name.to_string();
            let name = if name.len() == 1 {
                (name.remove(0) as u32).to_string()
            } else {
                format!("winuser::{}", name)
            };

            write!(outfile, "({}, {}) => Some(Key::{}),", name, extended, val).unwrap();
        } else if let (Some(name), Some(val)) = (d[0].as_str(), d[1].as_str()) {
            let mut name = name.to_string();
            let name = if name.len() == 1 {
                (name.remove(0) as u32).to_string()
            } else {
                format!("winuser::{}", name)
            };

            write!(outfile, "({}, _) => Some(Key::{}),", name, val).unwrap();
        }
    });

    outfile
        .write_all(
            b"        _ => None,
    }
}

#[inline]
pub fn convert_key_to_vkey(key: Key) -> c_int {
    match key {",
        )
        .unwrap();

    data.into_iter().for_each(|d| {
        let name = d[0].as_str();
        let val = d.get(2).unwrap_or(&d[1]).as_str();
        if let (Some(name), Some(val)) = (name, val) {
            let mut name = name.to_string();
            let name = if name.len() == 1 {
                (name.remove(0) as u32).to_string()
            } else {
                format!("winuser::{}", name)
            };

            write!(outfile, "Key::{} => {},", val, name).unwrap();
        }
    });

    outfile
        .write_all(
            b"
        _ => panic!(\"gluten_keyboard key does not map to winuser key\"),
    }
}
",
        )
        .unwrap();
}