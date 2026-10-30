//! Positioned reads: read bytes at an offset without moving a shared file
//! cursor, so the viewer, the index thread, and a search thread never race.

use std::fs::File;
use std::io;

/// Read into `buf` at `offset`, returning how many bytes were read (0 at EOF).
fn read_once(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    #[cfg(unix)]
    {
        std::os::unix::fs::FileExt::read_at(file, buf, offset)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::FileExt::seek_read(file, buf, offset)
    }
    #[cfg(not(any(unix, windows)))]
    {
        // Each reader opens its own handle, so seeking it is not shared.
        use io::{Read as _, Seek as _};
        let mut f = file;
        f.seek(io::SeekFrom::Start(offset))?;
        f.read(buf)
    }
}

/// Fill as much of `buf` as the file has from `offset`, returning the count
/// (short only at end of file).
///
/// # Errors
///
/// The underlying read error.
pub(crate) fn read_full(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    let mut got = 0;
    while got < buf.len() {
        match read_once(file, &mut buf[got..], offset + got as u64) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(got)
}
