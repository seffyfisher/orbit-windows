use std::{env, fs, path::PathBuf};
fn align(v: &mut Vec<u8>) {
    while !v.len().is_multiple_of(4) {
        v.push(0);
    }
}
fn wide(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .chain(Some(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}
fn res(out: &mut Vec<u8>, kind: u16, id: u16, data: &[u8]) {
    align(out);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&32u32.to_le_bytes());
    for n in [0xffff, kind, 0xffff, id] {
        out.extend_from_slice(&n.to_le_bytes());
    }
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0x30u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&[0; 8]);
    out.extend_from_slice(data);
    align(out);
}
fn block(key: &str, text: bool, value: &[u8], children: &[Vec<u8>]) -> Vec<u8> {
    let mut v = vec![0, 0];
    let len = if text { value.len() / 2 } else { value.len() } as u16;
    v.extend_from_slice(&len.to_le_bytes());
    v.extend_from_slice(&(text as u16).to_le_bytes());
    v.extend(wide(key));
    align(&mut v);
    v.extend_from_slice(value);
    for c in children {
        align(&mut v);
        v.extend_from_slice(c);
    }
    let len = v.len() as u16;
    v[..2].copy_from_slice(&len.to_le_bytes());
    v
}
fn main() {
    println!("cargo:rerun-if-env-changed=ORBIT_PAYLOAD");
    if let Ok(path) = env::var("ORBIT_PAYLOAD") {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-changed=gift.manifest");
    println!("cargo:rerun-if-changed=../assets/orbit.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut bytes = Vec::new();
    res(&mut bytes, 0, 0, &[]);
    res(
        &mut bytes,
        24,
        1,
        &fs::read("gift.manifest").expect("manifest"),
    );
    let ico = fs::read("../assets/orbit.ico").expect("app icon");
    let count = u16::from_le_bytes([ico[4], ico[5]]);
    let mut group = ico[..6].to_vec();
    for i in 0..count as usize {
        let p = 6 + i * 16;
        let size = u32::from_le_bytes(ico[p + 8..p + 12].try_into().unwrap()) as usize;
        let offset = u32::from_le_bytes(ico[p + 12..p + 16].try_into().unwrap()) as usize;
        res(&mut bytes, 3, i as u16 + 1, &ico[offset..offset + size]);
        group.extend_from_slice(&ico[p..p + 12]);
        group.extend_from_slice(&(i as u16 + 1).to_le_bytes());
    }
    res(&mut bytes, 14, 1, &group);
    let strings: Vec<_> = [
        ("FileDescription", "Orbit gift installer"),
        ("FileVersion", "0.1.0.0"),
        ("ProductName", "Orbit"),
        ("ProductVersion", "0.1.0"),
        ("OriginalFilename", "Orbit-Gift-0.1.0-x64.exe"),
        ("LegalCopyright", "Copyright 2026 Seffy Fisher"),
    ]
    .iter()
    .map(|(k, v)| block(k, true, &wide(v), &[]))
    .collect();
    let fixed: Vec<u8> = [
        0xfeef04bdu32,
        0x10000,
        0x00000001,
        0,
        0x00000001,
        0,
        0x3f,
        0,
        0x40004,
        1,
        0,
        0,
        0,
    ]
    .iter()
    .flat_map(|n| n.to_le_bytes())
    .collect();
    res(
        &mut bytes,
        16,
        1,
        &block(
            "VS_VERSION_INFO",
            false,
            &fixed,
            &[
                block(
                    "StringFileInfo",
                    true,
                    &[],
                    &[block("040904b0", true, &[], &strings)],
                ),
                block(
                    "VarFileInfo",
                    true,
                    &[],
                    &[block("Translation", false, &[9, 4, 0xb0, 4], &[])],
                ),
            ],
        ),
    );
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("orbit.res");
    fs::write(&out, bytes).unwrap();
    println!("cargo:rustc-link-arg-bin=orbit-gift={}", out.display());
}
