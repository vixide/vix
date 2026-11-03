//! Positioned reads: read bytes at an offset without moving a shared file
//! cursor, so the viewer, the index thread, and a search thread never race.

use std::fs::File;
use std::io;

/// One positioned read (may be short). Platform split: `pread`-style
/// `read_at` on Unix, `seek_read` on Windows (which does move that handle's
/// cursor, but no code here relies on the cursor), and a seek+read fallback
/// elsewhere that is only safe because every reader owns its own handle.
///
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
    // `read_at` may return short counts, so loop until the buffer is full or
    // EOF (`Ok(0)`); `Interrupted` (EINTR) is retried, never surfaced.
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
