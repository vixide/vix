//! Helpers shared by the unit tests.

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// Write `bytes` to a fresh file in the temp directory.
pub(crate) fn temp_file(bytes: &[u8]) -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("vix-large-file-unit-{}-{seq}", std::process::id()));
    std::fs::File::create(&path)
        .unwrap()
        .write_all(bytes)
        .unwrap();
    path
}

/// `len` pseudo-random bytes from `seed`: about one in `density` is a `\n`,
/// some are `\r`, and the rest are any byte (so invalid UTF-8 too). Cheap to
/// generate even at hundreds of kilobytes, unlike a per-byte strategy.
pub(crate) fn random_text(len: usize, seed: u64, density: u64) -> Vec<u8> {
    let mut x = seed | 1;
    let density = density.max(1);
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            if x.is_multiple_of(density) {
                b'\n'
            } else if x.is_multiple_of(97) {
                b'\r'
            } else {
                x.to_le_bytes()[3]
            }
        })
        .collect()
}
