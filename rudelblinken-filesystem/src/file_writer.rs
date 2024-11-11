use std::{
    io::{Seek, SeekFrom, Write},
    isize,
    sync::{Arc, RwLock},
};

use crate::storage::Storage;

/// Enables you to write a file directly to flash without using any additional memory
///
/// Use the Write and Seek implementations to write the file.
///
/// Keep in mind that write can not reset bits to 0 but only set them to 1. This should not be a problem as all bits are guaranteed to be set to 0 on a new FileWriter.
///
/// The file is considered invalid until you [complete] this filewriter. You cant edit it afterwards.
///
/// If you drop the FileWriter without calling finalize, the file will be deleted.
pub struct FileWriter<T: Storage + 'static> {
    storage: Arc<RwLock<T>>,
    start_position: usize,
    length: usize,
    current_offset: usize,
}

impl<T: Storage + 'static> FileWriter<T> {
    pub fn commit(self) {}

    pub fn new(storage: Arc<RwLock<T>>, address: usize, length: usize) -> Self {
        return FileWriter::<T> {
            storage,
            start_position: address,
            length: length,
            current_offset: 0,
        };
    }
}

impl<T: Storage + 'static> Seek for FileWriter<T> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let new_offset = match pos {
            SeekFrom::Start(offset) => offset
                .try_into()
                .unwrap_or(std::usize::MAX)
                .clamp(0usize, self.length),
            SeekFrom::End(offset) => self
                .length
                .saturating_add_signed(
                    offset
                        .clamp(isize::MIN as i64, isize::MAX as i64)
                        .try_into()
                        .unwrap(),
                )
                .clamp(0usize, self.length),
            SeekFrom::Current(offset) => self
                .current_offset
                .saturating_add_signed(
                    offset
                        .clamp(isize::MIN as i64, isize::MAX as i64)
                        .try_into()
                        .unwrap(),
                )
                .clamp(0usize, self.length),
        };

        self.current_offset = new_offset;
        return Ok(self.current_offset as u64);
    }
}

impl<T: Storage + 'static> Write for FileWriter<T> {
    /// The same as [std::io::Write::write] but you can only flip bits from 0 to 1
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let remaining_length = self.length.saturating_sub(self.current_offset);
        let write_length = std::cmp::min(remaining_length, buf.len());

        let mut writable_storage = self
            .storage
            .write()
            .map_err(|e| std::io::ErrorKind::ResourceBusy)?;
        writable_storage
            .write(
                self.start_position + self.current_offset,
                &buf[0..write_length],
            )
            .map_err(|e| std::io::Error::other(e))?;
        self.current_offset += write_length;
        return Ok(write_length);
    }

    fn flush(&mut self) -> std::io::Result<()> {
        return Ok(());
    }
}

impl<T: Storage + 'static> Drop for FileWriter<T> {
    fn drop(&mut self) {
        // TODO: Make sure the file gets deleted if not committed
    }
}
