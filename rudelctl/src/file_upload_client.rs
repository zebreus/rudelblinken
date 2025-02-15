//! Connects to our Bluetooth GATT service and exercises the characteristic.
use crate::GLOBAL_LOGGER;
use async_recursion::async_recursion;
use bluer::{
    gatt::remote::{Characteristic, CharacteristicWriteRequest},
    Device, UuidExt,
};
use futures::{join, lock::Mutex, StreamExt};
use helpers::{
    connect_to_device, find_characteristic, find_service, FindCharacteristicError, FindServiceError,
};
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use rand::{distributions::Alphanumeric, Rng};
use std::{fmt::Write, ops::Div, pin::pin, sync::Arc, time::Duration};
use thiserror::Error;
use tokio::{
    io::{stdin, AsyncReadExt, AsyncWriteExt},
    time::sleep,
};
use tokio_util::sync::CancellationToken;
use upload_request::UploadRequest;
use uuid::Uuid;
use zerocopy::IntoBytes;
mod helpers;
mod upload_request;

const FILE_UPLOAD_SERVICE: u16 = 0x9160;
// Write data chunks here
const FILE_UPLOAD_SERVICE_DATA: u16 = 0x9161;
// Write metadata here to initiate an upload. Returns the metadata of the current upload
const FILE_UPLOAD_SERVICE_START_UPLOAD: u16 = 0x9162;
// Read this to get the number of uploaded chunks and the IDs of some missing chunks. Returns a list of u16
const FILE_UPLOAD_SERVICE_UPLOAD_PROGRESS: u16 = 0x9163;
// Read here to get the last error as a string
const FILE_UPLOAD_SERVICE_LAST_ERROR: u16 = 0x9164;
// Read to get the hash of the current upload.
const FILE_UPLOAD_SERVICE_CURRENT_HASH: u16 = 0x9166;

const CAT_MANAGEMENT_SERVICE: u16 = 0x7992;
const CAT_MANAGEMENT_SERVICE_PROGRAM_HASH: u16 = 0x7893;
const CAT_MANAGEMENT_SERVICE_NAME: u16 = 0x7894;

const SERIAL_LOGGING_TIO_SERVICE: Uuid = uuid::uuid!("6E400001-B5A3-F393-E0A9-E50E24DCCA9E");
const SERIAL_LOGGING_TIO_CHAR_RX: Uuid = uuid::uuid!("6E400002-B5A3-F393-E0A9-E50E24DCCA9E"); // Write no response
const SERIAL_LOGGING_TIO_CHAR_TX: Uuid = uuid::uuid!("6E400003-B5A3-F393-E0A9-E50E24DCCA9E"); // Notify

#[derive(Error, Debug)]
pub enum UpdateTargetError {
    #[error("BlueR error")]
    BluerError(#[from] bluer::Error),
    #[error("io error")]
    IoError(#[from] std::io::Error),
    #[error("Not an update target")]
    MacDoesNotLookLikeAnUploadServiceProvider,
    #[error("Failed to connect to device")]
    FailedToConnect(bluer::Error),
    #[error(transparent)]
    DoesNotProvideUpdateService(#[from] FindServiceError),
    #[error(transparent)]
    ServiceIsMissingACharacteristic(#[from] FindCharacteristicError),
    #[error("Failed to upload file. Maybe a timeout or connection loss: {0}")]
    UploadError(bluer::Error),
    #[error("The update target seemingly ignored our upload request")]
    UploadRequestIgnored,
    #[error("We lost connection to the target device and failed to reconnect")]
    ReconnectFailed,
    #[error("The upload status did not contain the current progress")]
    FailedToParseUploadStatus,
}

pub struct FileUploadClient {
    data_characteristic: Characteristic,
    start_upload_characteristic: Characteristic,
    missing_chunks_characteristic: Characteristic,
    // TODO: Use this
    #[allow(dead_code)]
    last_error_characteristic: Characteristic,
    current_hash_characteristic: Characteristic,

    log_tx_characteristic: Characteristic,
    log_rx_characteristic: Characteristic,

    program_hash_characteristic: Characteristic,
    name_characteristic: Characteristic,
    device: Device,
}

impl FileUploadClient {
    pub async fn new_from_peripheral(
        device: &Device,
    ) -> Result<FileUploadClient, UpdateTargetError> {
        let device = device.clone();
        let address = device.address();
        log::debug!("Checking {}", address);
        if !(address.0.starts_with(&[0x24, 0xec, 0x4b])) {
            return Err(UpdateTargetError::MacDoesNotLookLikeAnUploadServiceProvider);
        }
        log::debug!("Found MAC {}", address);

        connect_to_device(&device).await?;

        let update_service =
            find_service(&device, uuid::Uuid::from_u16(FILE_UPLOAD_SERVICE)).await?;

        let data_characteristic = find_characteristic(
            &update_service,
            uuid::Uuid::from_u16(FILE_UPLOAD_SERVICE_DATA),
        )
        .await?;
        let start_upload_characteristic = find_characteristic(
            &update_service,
            uuid::Uuid::from_u16(FILE_UPLOAD_SERVICE_START_UPLOAD),
        )
        .await?;
        let missing_chunks_characteristic = find_characteristic(
            &update_service,
            uuid::Uuid::from_u16(FILE_UPLOAD_SERVICE_UPLOAD_PROGRESS),
        )
        .await?;
        let last_error_characteristic = find_characteristic(
            &update_service,
            uuid::Uuid::from_u16(FILE_UPLOAD_SERVICE_LAST_ERROR),
        )
        .await?;
        let current_hash_characteristic = find_characteristic(
            &update_service,
            uuid::Uuid::from_u16(FILE_UPLOAD_SERVICE_CURRENT_HASH),
        )
        .await?;

        let cat_management_service =
            find_service(&device, uuid::Uuid::from_u16(CAT_MANAGEMENT_SERVICE)).await?;

        let name_characteristic = find_characteristic(
            &cat_management_service,
            uuid::Uuid::from_u16(CAT_MANAGEMENT_SERVICE_NAME),
        )
        .await?;
        let program_hash_characteristic = find_characteristic(
            &cat_management_service,
            uuid::Uuid::from_u16(CAT_MANAGEMENT_SERVICE_PROGRAM_HASH),
        )
        .await?;

        let logging_service = find_service(&device, SERIAL_LOGGING_TIO_SERVICE).await?;
        let log_tx_characteristic =
            find_characteristic(&logging_service, SERIAL_LOGGING_TIO_CHAR_TX).await?;
        let log_rx_characteristic =
            find_characteristic(&logging_service, SERIAL_LOGGING_TIO_CHAR_RX).await?;

        return Ok(FileUploadClient {
            data_characteristic,
            start_upload_characteristic,
            missing_chunks_characteristic,
            last_error_characteristic,
            name_characteristic,
            program_hash_characteristic,
            current_hash_characteristic,
            log_tx_characteristic,
            log_rx_characteristic,
            device,
        });
    }

    pub async fn get_name(&self) -> Result<String, UpdateTargetError> {
        let name_bytes = self.name_characteristic.read().await?;
        if name_bytes.len() < 3 || name_bytes.len() > 32 {
            todo!();
        }
        let name = String::from_utf8_lossy(&name_bytes);
        return Ok(name.to_string());
    }

    pub async fn run_program(&self, data: &[u8]) -> Result<(), UpdateTargetError> {
        let file_name: Vec<u8> = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(10)
            .collect();
        let file_name = String::from_utf8(file_name).unwrap();
        let program_hash = self.upload_file(data, file_name).await?;
        log::debug!("Uploaded file.");
        self.program_hash_characteristic
            .write_ext(
                &program_hash,
                &CharacteristicWriteRequest {
                    offset: 0,
                    op_type: bluer::gatt::WriteOp::Reliable,
                    prepare_authorize: false,
                    _non_exhaustive: (),
                },
            )
            .await?;
        log::debug!("Wrote program hash.");
        return Ok(());
    }

    #[async_recursion(?Send)]
    pub async fn upload_file(
        &self,
        data: &[u8],
        file_name: String,
    ) -> Result<[u8; 32], UpdateTargetError> {
        log::debug!("Preparing data for upload...");

        // -2 for the length
        // -28 was found to be good by empirical methods
        let chunk_size: u16 = (self.data_characteristic.mtu().await? as u16) - 28 - 2;
        log::debug!("Using a chunk size of {}", chunk_size);
        let chunks: Vec<Vec<u8>> = data
            .chunks(chunk_size as usize)
            .enumerate()
            .map(|(index, data)| {
                let mut new_chunk = vec![0; data.len() + 2];
                new_chunk[0..2].copy_from_slice(&(index as u16).to_le_bytes());
                new_chunk[2..(2 + data.len())].copy_from_slice(data);
                return new_chunk;
            })
            .collect();

        // TODO: Fix the name story on both sides.
        // file_name[0..9].copy_from_slice(&"test.wasm".as_bytes());

        let upload_request = UploadRequest::new(&file_name, data, chunk_size, async |data| {
            self.upload_file(data, "checksums.temp".into()).await
        })
        .await?;

        self.start_upload(&upload_request).await?;
        self.upload_chunks(chunks).await?;
        log::debug!("Uploaded file {:?}", upload_request.hash);
        return Ok(upload_request.hash);
    }

    async fn start_upload(&self, upload_request: &UploadRequest) -> Result<(), UpdateTargetError> {
        let upload_request_bytes = upload_request.as_bytes();
        log::debug!("Sending file information...");

        self.start_upload_characteristic
            .write(&upload_request_bytes)
            .await?;

        const MAX_RETRIES: usize = 10;
        let mut retries_left = MAX_RETRIES;
        loop {
            let current_target_hash = self.current_hash_characteristic.read().await?;
            if current_target_hash == upload_request.hash {
                break;
            }

            if retries_left == 0 {
                return Err(UpdateTargetError::UploadRequestIgnored);
            }
            log::debug!(
                "Target did not process our upload request. Retry {}/{}...",
                MAX_RETRIES - retries_left,
                MAX_RETRIES
            );
            retries_left -= 1;
            self.start_upload_characteristic
                .write(&upload_request_bytes)
                .await?;
            sleep(Duration::from_secs(1)).await;
        }
        Ok(())
    }

    async fn upload_chunks(&self, chunks: Vec<Vec<u8>>) -> Result<(), UpdateTargetError> {
        // Chunk size without the index
        let chunk_size = chunks.first().map_or(0, |chunk| chunk.len() - 2);
        // Total size without the indexes
        let total_size = chunks.iter().map(|chunk| chunk.len() - 2).sum::<usize>() as u64;
        let progress_bar = GLOBAL_LOGGER.add(ProgressBar::new(total_size));
        // let progress_bar = ProgressBar::new(chunks.len() as u64);
        progress_bar.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg:20}")
          .unwrap()
          .with_key("eta", |state: &ProgressState, w: &mut dyn Write| write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap())
          .progress_chars("#>-"));
        progress_bar.set_message("starting");
        progress_bar.enable_steady_tick(Duration::from_millis(100));
        let progress_bar_arc = Arc::new(Mutex::new(progress_bar));

        // The number of chunks we send between checking for missing chunks
        // The read after the write will wait until this number of chunks is written. If we send too many chunks at once, we get timeouts
        let mut simultaneous_chunks = 2usize;
        // The smallest number of chunks, where we had a bad transfer
        let mut min_bad_chunks = 1000usize;
        // let mut max_good_chunks = 1;
        // How many times we will reconnect to the device
        let total_reconnects = 10usize;
        let mut reconnects_left = 10usize;
        let mut estimated_speed = Duration::from_secs(1);
        let mut measurement_valid = false;
        let mut last_transfer_start = std::time::Instant::now();
        let mut last_transfer_chunks = 1usize;
        let mut cancel_auto_increment = CancellationToken::new();
        loop {
            // Reading a property will wait until the writes are done
            let upload_status = match self.missing_chunks_characteristic.read().await {
                Ok(upload_status) => upload_status,
                Err(error) => {
                    measurement_valid = false;
                    let progress_bar = progress_bar_arc.lock().await;
                    progress_bar.set_message("error");
                    // // Does not seem to work
                    // let is_connected = self.device.is_connected().await?;
                    let error_message_looks_like_connection_error =
                        error.to_string().contains("connect")
                            || error.to_string().contains("reset")
                            || error.to_string().contains("present")
                            || error.to_string().contains("removed");

                    if error_message_looks_like_connection_error {
                        if reconnects_left == 0 {
                            log::info!("Reconnect failed. Aborting upload.");
                            progress_bar.abandon_with_message("reconnect failed");
                            GLOBAL_LOGGER.remove(&progress_bar);

                            return Err(UpdateTargetError::ReconnectFailed);
                        }
                        log::info!(
                            "Connection lost. Attempting reconnect {:>2}/{:2}...",
                            reconnects_left,
                            total_reconnects
                        );
                        progress_bar.set_message(format!(
                            "reconnect {:>2}/{:2}",
                            reconnects_left, total_reconnects
                        ));
                        drop(progress_bar);
                        let _ = self.device.connect().await;
                        sleep(Duration::from_secs(2)).await;
                        reconnects_left -= 1;
                        continue;
                    }

                    min_bad_chunks = std::cmp::min(last_transfer_chunks, min_bad_chunks);
                    log::debug!("Failed to read missing chunks: {}", error);
                    let new_simultaneous_chunks =
                        std::cmp::max(1, last_transfer_chunks.div_floor(2));
                    log::info!("Failed to transfer chunks. Reducing the number of chunks per transfer to {}", new_simultaneous_chunks);
                    progress_bar
                        .set_message(format!("retry with size {}", new_simultaneous_chunks));
                    if new_simultaneous_chunks == 1 {
                        reconnects_left = reconnects_left.saturating_sub(1);
                        if reconnects_left == 0 {
                            progress_bar.abandon_with_message("upload failed");
                            GLOBAL_LOGGER.remove(&progress_bar);

                            return Err(UpdateTargetError::UploadError(error));
                        }
                    }
                    drop(progress_bar);

                    sleep(Duration::from_secs(3)).await;

                    simultaneous_chunks = new_simultaneous_chunks;
                    continue;
                }
            };
            let progress_bar = progress_bar_arc.lock().await;
            if measurement_valid {
                let last_transfer_duration = last_transfer_start.elapsed();
                estimated_speed = last_transfer_duration.div(last_transfer_chunks as u32);

                simultaneous_chunks = std::cmp::max(
                    1,
                    std::cmp::min(
                        min_bad_chunks - 1,
                        simultaneous_chunks + simultaneous_chunks.div_ceil(3),
                    ),
                );
            }

            let upload_status = upload_status
                .array_chunks::<2>()
                .map(|chunk_id_bytes| u16::from_le_bytes(*chunk_id_bytes))
                .collect::<Vec<u16>>();
            if upload_status.len() <= 1 {
                break;
            }
            let Some(([transferred_chunks], missing_chunks)) = upload_status.split_at_checked(1)
            else {
                progress_bar.abandon_with_message("failed to parse upload status");
                GLOBAL_LOGGER.remove(&progress_bar);

                return Err(UpdateTargetError::FailedToParseUploadStatus);
            };

            // The number of chunks that will be uploaded this transfer
            let number_of_chunks = std::cmp::min(missing_chunks.len(), simultaneous_chunks);
            log::info!("Transferring {} chunks", number_of_chunks);
            cancel_auto_increment.cancel();
            progress_bar.set_message("active");
            progress_bar.set_position(std::cmp::min(
                total_size,
                *transferred_chunks as u64 * chunk_size as u64,
            ));
            progress_bar.enable_steady_tick(Duration::from_millis(100));
            drop(progress_bar);
            log::debug!("Transferring the following chunks: {:?}", missing_chunks);

            cancel_auto_increment = CancellationToken::new();
            let cloned_token = cancel_auto_increment.clone();
            let cloned_progress_bar = progress_bar_arc.clone();
            tokio::spawn(async move {
                for _ in 0..number_of_chunks {
                    sleep(estimated_speed).await;
                    let progress_bar = cloned_progress_bar.lock().await;
                    if cloned_token.is_cancelled() {
                        return;
                    }
                    let previous_value = progress_bar.position();
                    progress_bar.set_position(previous_value + chunk_size as u64);
                }
                cloned_progress_bar.lock().await.set_message("waiting");
            });
            last_transfer_start = std::time::Instant::now();
            last_transfer_chunks = number_of_chunks;
            measurement_valid = true;

            // Upload at most 10 chunks at a time, because we may get timeouts otherwise
            let mut write_io = self.data_characteristic.write_io().await?;
            for chunk_id in missing_chunks.iter().take(number_of_chunks) {
                write_io.send(&chunks[*chunk_id as usize]).await.unwrap();
            }
            write_io.flush().await.unwrap();
        }
        log::info!("File uploaded successfully.");
        let progress_bar = progress_bar_arc.lock().await;
        progress_bar.finish_with_message("uploaded");
        GLOBAL_LOGGER.remove(&progress_bar);

        Ok(())
    }

    pub async fn attach_logger(&self) -> Result<(), UpdateTargetError> {
        let log_receiver = self.log_tx_characteristic.notify();
        let mut log_receiver = pin!(log_receiver.await?);

        let printer = async {
            while let Some(chunk) = log_receiver.next().await {
                let Ok(chunk) = std::str::from_utf8(chunk.as_ref()) else {
                    log::warn!("Received log message contains invalid UTF-8. Not printing it.");
                    // TODO: Handle unicode characters split across multiple messages
                    continue;
                };
                print!("{}", chunk);
            }
            return Result::<(), UpdateTargetError>::Ok(());
        };

        let reader = async {
            let mut buffer = [0u8; 200];
            while let Ok(length) = stdin().read(&mut buffer).await {
                let result = self.log_rx_characteristic.write(&buffer[0..length]).await;
                if result.is_err() {
                    log::error!("Failed to send input to client");
                    // TODO: Implement retry mechanism
                    return result;
                }
            }
            return Ok(());
        };

        // TODO: Use select instead of join
        let (writer_result, reader_result) = join!(printer, reader);
        reader_result?;
        writer_result?;
        Ok(())
    }
}
