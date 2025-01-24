use crate::storage::FlashStorage;
use itertools::Itertools;
use rudelblinken_filesystem::{
    file::{File as FileContent, FileState},
    Filesystem,
};
use std::io::{Seek, Write};
use thiserror::Error;

#[derive(Debug)]
pub(super) struct IncompleteFile {
    incomplete_file: FileContent<FlashStorage, { FileState::Writer }>,
    checksums: Vec<u8>,
    received_chunks: Vec<bool>,
    chunk_length: u16,
    length: u32,
    name: String,
    hash: [u8; 32],
}

#[derive(Error, Debug, Clone)]
pub enum ReceiveChunkError {
    #[error("Chunk has an invalid length")]
    InvalidLength,
    #[error("Chunk has the wrong checksum")]
    WrongChecksum,
}

#[derive(Error, Debug, Clone)]
pub enum VerifyFileError {
    #[error("File is not complete")]
    NotComplete,
    #[error("Hashes do not match")]
    HashMismatch,
}

impl IncompleteFile {
    pub fn new(
        hash: [u8; 32],
        checksums: Vec<u8>,
        chunk_length: u16,
        length: u32,
        writer: FileContent<FlashStorage, { FileState::Writer }>,
        name: String,
    ) -> Self {
        Self {
            incomplete_file: writer,
            received_chunks: vec![false; checksums.len()],
            checksums,
            chunk_length,
            length,
            name,
            hash,
        }
    }

    pub fn receive_chunk(&mut self, data: &[u8], index: u16) -> Result<(), ReceiveChunkError> {
        // Verify length for all but the last chunk
        if (index as usize != self.checksums.len() - 1)
            && (data.len() != self.chunk_length as usize)
        {
            return Err(ReceiveChunkError::InvalidLength);
        }
        // Verify length for the last chunk
        if (index as usize == self.checksums.len() - 1)
            && (data.len() != (self.length as usize % self.chunk_length as usize))
        {
            return Err(ReceiveChunkError::InvalidLength);
        }

        // TODO: Find out if generating a new crc8 generator costs anything
        let crc8_generator = crc::Crc::<u8>::new(&crc::CRC_8_LTE);
        let checksum = crc8_generator.checksum(data);

        if self.checksums[index as usize] != checksum {
            ::tracing::error!(target: "file-upload", "Received chunk with invalid checksum");
            return Err(ReceiveChunkError::WrongChecksum);
        }

        let offset = self.chunk_length as usize * index as usize;
        self.incomplete_file
            .seek(std::io::SeekFrom::Start(offset as u64))
            .unwrap();
        self.incomplete_file.write(data).unwrap();
        // self.incomplete_file.content[offset..(data.len() + offset)].copy_from_slice(data);
        self.received_chunks[index as usize] = true;

        Ok(())
    }
    /// Get all chunks that have not yet been received
    pub fn get_missing_chunks(&self) -> Vec<u16> {
        self.received_chunks
            .iter()
            .enumerate()
            .filter(|(_, received)| received == &&false)
            .map(|(index, _)| index as u16)
            .collect_vec()
    }
    /// Check if the file is complete
    pub fn is_complete(&self) -> bool {
        self.received_chunks.iter().all(|received| *received)
    }
    /// Verify that the received file is complete and has the correct hash
    pub fn verify_hash(
        self,
        filesystem: &Filesystem<FlashStorage>,
    ) -> Result<FileContent<FlashStorage, { FileState::Weak }>, VerifyFileError> {
        if !self.is_complete() {
            return Err(VerifyFileError::NotComplete);
        }
        self.incomplete_file.commit().unwrap();
        let file = filesystem.read_file(&self.name).unwrap();
        let mut hasher = blake3::Hasher::new();
        hasher.update(file.upgrade().unwrap().as_ref());

        // TODO: I am sure there is a better way to convert this into an array but I didnt find it after 10 minutes.
        let mut hash: [u8; 32] = [0; 32];
        hash.copy_from_slice(hasher.finalize().as_bytes());

        if hash != self.hash {
            ::tracing::warn!(target: "file-upload", "Hashes dont match.\nExpected: {:?}\nGot     : {:?}", self.hash, hash);
            return Err(VerifyFileError::HashMismatch);
        }
        ::tracing::info!(target: "file-upload", "Hashes match");

        Ok(file)
    }
    /// Get the uploaded file, if the upload is finished, otherwise this return None and you just destroyed your incomplete file for no reason
    pub fn into_file(
        self,
        filesystem: &Filesystem<FlashStorage>,
    ) -> Result<FileContent<FlashStorage, { FileState::Weak }>, VerifyFileError> {
        let file = self.verify_hash(filesystem)?;
        Ok(file)
    }

    pub fn get_hash(&self) -> &[u8; 32] {
        &self.hash
    }

    pub fn get_status(&self) -> (u16, Vec<u16>) {
        let missing_chunks = self.get_missing_chunks();
        let progress = self.received_chunks.len() as u16 - missing_chunks.len() as u16;
        (progress, missing_chunks)
    }
}
