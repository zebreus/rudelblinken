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
/// The file is considered invalid until you [commit] this filewriter. You cant edit it afterwards.
///
/// If you drop the FileWriter without calling [commit], the file will be deleted.
pub struct FileWriter<T: Storage + 'static> {
    storage: Arc<RwLock<T>>,
    start_position: u32,
    length: u32,
    current_offset: u32,
    /// Destructor that will be called when the last strong reference is dropped
    ///
    /// The first argument is whether the file was committed (true) or not (false)
    destructor: Box<dyn FnOnce(bool) -> () + 'static>,
    committed: bool,
}

impl<T: Storage + 'static> FileWriter<T> {
    pub fn new(
        storage: Arc<RwLock<T>>,
        address: u32,
        length: u32,
        destructor: impl FnOnce(bool) -> () + 'static,
    ) -> Self {
        return FileWriter::<T> {
            storage,
            start_position: address,
            length: length,
            current_offset: 0,
            destructor: Box::new(destructor),
            committed: false,
        };
    }

    pub fn commit(mut self) {
        // Set committed to true and drop self
        self.committed = true;
        // Be explicit about drop
        drop(self);
    }
}

impl<T: Storage + 'static> Seek for FileWriter<T> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let new_offset = match pos {
            SeekFrom::Start(offset) => offset
                .try_into()
                .unwrap_or(std::u32::MAX)
                .clamp(0, self.length),
            SeekFrom::End(offset) => self
                .length
                .saturating_add_signed(
                    offset
                        .clamp(isize::MIN as i64, isize::MAX as i64)
                        .try_into()
                        .unwrap(),
                )
                .clamp(0, self.length),
            SeekFrom::Current(offset) => self
                .current_offset
                .saturating_add_signed(
                    offset
                        .clamp(isize::MIN as i64, isize::MAX as i64)
                        .try_into()
                        .unwrap(),
                )
                .clamp(0, self.length),
        };

        self.current_offset = new_offset;
        return Ok(self.current_offset as u64);
    }
}

impl<T: Storage + 'static> Write for FileWriter<T> {
    /// The same as [std::io::Write::write] but you can only flip bits from 0 to 1
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let remaining_length = self.length.saturating_sub(self.current_offset);
        let write_length = std::cmp::min(remaining_length, buf.len() as u32);

        let mut writable_storage = self
            .storage
            .write()
            .map_err(|e| std::io::ErrorKind::ResourceBusy)?;
        writable_storage
            .write(
                self.start_position + self.current_offset,
                &buf[0..write_length as usize],
            )
            .map_err(|e| std::io::Error::other(e))?;
        self.current_offset += write_length;
        return Ok(write_length as usize);
    }

    fn flush(&mut self) -> std::io::Result<()> {
        return Ok(());
    }
}

impl<T: Storage + 'static> Drop for FileWriter<T> {
    fn drop(&mut self) {
        let previous_destructor: &mut Box<dyn FnOnce(bool) -> ()> = &mut self.destructor;
        let empty_destructor: Box<dyn FnOnce(bool) -> ()> = Box::new(|_| ());
        let destructor = std::mem::replace(previous_destructor, empty_destructor);
        (destructor)(self.committed);

        // TODO: Make sure the file gets deleted if not committed
    }
}
