// This file exists twice, once here and once in rudelctl
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes};

// TODO: Implement better debug printing
#[derive(Debug, Clone, TryFromBytes, IntoBytes, Immutable, KnownLayout, PartialEq, PartialOrd)]
#[repr(C)]
pub struct UploadRequest {
    /// Size of the file in bytes
    pub file_size: u32,
    /// Blake3 hash of the file
    pub hash: [u8; 32],
    /// CRC checksums of the chunks.
    /// If the number of chunks is <= 32 then this is interpreted as an array of 1-byte CRC checksums
    /// If the number of chunks is > 32 then this interpreted as the hash of a previously uploaded file containing an array of 1-byte CRC checksums
    pub checksums: [u8; 32],
    /// File name
    pub file_name: [u8; 16],
    /// Size of a single chunk
    pub chunk_size: u16,
    /// Unused padding. Reserved for future use
    pub _padding: u16,
}

impl UploadRequest {
    // Get the total number of chunks
    pub fn chunk_count(&self) -> u32 {
        self.file_size.div_ceil(self.chunk_size as u32)
    }
}
