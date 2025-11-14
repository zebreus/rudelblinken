use bluer::monitor::{Monitor, MonitorEvent, Pattern, RssiSamplingPeriod, Type as MonitorType};
use bluer::DiscoveryFilter;
use futures::{
    pin_mut,
    stream::{AbortHandle, Abortable},
    StreamExt as STTT,
};
use futures_time::stream::StreamExt;
use futures_time::time::Duration;
use std::{collections::HashSet, future::Future};

#[derive(Debug)]
pub enum Outcome {
    Processed,
    Ignored,
}

pub async fn scan_for<Fut, Err>(
    duration: Duration,
    // Just give a big number if you dont want a limit
    max_devices: u32,
    name_filter: impl Fn(&str) -> bool,
    // Power cycle the adapter to make discovery more reliable
    // TODO: Find a better fix
    powercycle_adapter: bool,
    f: &dyn Fn(bluer::Device, AbortHandle) -> Fut,
) -> bluer::Result<()>
where
    Err: std::fmt::Debug,
    Fut: Future<Output = Result<Outcome, Err>>,
{
    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;

    // Power cycle the adapter to make discovery more reliable
    if powercycle_adapter {
        let _ = adapter.set_powered(false).await;
    }
    adapter.set_powered(true).await?;

    /// !! AI Warning OwO (generated and untested, likely broken)
    /*  // Try Advertisement Monitor first (more reliable and passive on BlueZ)
    if let Ok(mm) = adapter.monitor().await {
        let name_prefix = b"[rb]".to_vec();
        let pattern_complete = Pattern {
            data_type: 0x09,
            start_position: 0,
            content: name_prefix.clone(),
        };
        let pattern_short = Pattern {
            data_type: 0x08,
            start_position: 0,
            content: name_prefix.clone(),
        };
        let mut handle = mm
            .register(Monitor {
                monitor_type: MonitorType::OrPatterns,
                rssi_low_threshold: None,
                rssi_high_threshold: None,
                rssi_low_timeout: None,
                rssi_high_timeout: None,
                rssi_sampling_period: Some(RssiSamplingPeriod::First),
                patterns: Some(vec![pattern_complete, pattern_short]),
                ..Default::default()
            })
            .await?;

        let (abort_handle, abort_registration) = AbortHandle::new_pair();
        let stream = Abortable::new(&mut handle, abort_registration);
        pin_mut!(stream);
        let mut stream = stream.timeout(duration);

        let mut programmed_devices = 0u32;
        while let Some(evt) = stream.next().await {
            let Ok(evt) = evt else {
                break;
            };
            if let MonitorEvent::DeviceFound(devid) = evt {
                let device = adapter.device(devid.device)?;
                // Already filtered by AD name pattern; try processing immediately
                let result = f(device, abort_handle.clone()).await;
                if let Err(error) = result {
                    let string_error = format!("{:?}", error);
                    if !string_error.contains("TargetDoesNotLookLikeAnUploadServiceProvider") {
                        log::error!("Failed processing device with {:?}", string_error);
                    }
                    continue;
                }
                if let Ok(Outcome::Processed) = result {
                    programmed_devices += 1;
                    if programmed_devices >= max_devices {
                        abort_handle.abort();
                        break;
                    }
                }
            }
        }
        drop(handle);
        return Ok(());
    } */
    // Fallback: classic discovery stream
    let filter = DiscoveryFilter {
        uuids: HashSet::new(),
        rssi: None,
        pathloss: None,
        transport: bluer::DiscoveryTransport::Le,
        duplicate_data: true,
        discoverable: false,
        pattern: Some("[rb]".to_string()),
        _non_exhaustive: (),
    };
    // This is allowed to fail, as filters are not reliable anyways
    let _ = adapter.set_discovery_filter(filter).await;

    // Starts a discovery session
    // Monitor would be way more appropriate here, but that requires the user to enable experimental features in their bluetoothd
    let discover = adapter.discover_devices().await?;
    pin_mut!(discover);
    let (abort_handle, abort_registration) = AbortHandle::new_pair();
    let stream = Abortable::new(discover, abort_registration);
    let mut stream = stream.timeout(duration);
    let mut programmed_devices = 0;
    while let Some(evt) = stream.next().await {
        let Ok(evt) = evt else {
            break;
        };
        match evt {
            bluer::AdapterEvent::PropertyChanged(a) => {
                log::debug!("Adapter property changed: {:?}", a);
            }
            bluer::AdapterEvent::DeviceRemoved(addr) => {
                log::debug!("Device removed: {:?}", addr);
            }
            bluer::AdapterEvent::DeviceAdded(addr) => {
                let device = adapter.device(addr)?;
                let Some(name) = device.name().await? else {
                    continue;
                };
                if !name_filter(&name) {
                    continue;
                }

                let result = f(device, abort_handle.clone()).await;
                if let Err(error) = result {
                    let string_error = format!("{:?}", error);
                    if !string_error.contains("TargetDoesNotLookLikeAnUploadServiceProvider") {
                        log::error!("Failed processing device with {:?}", string_error);
                    }
                    continue;
                }
                if let Ok(Outcome::Processed) = result {
                    programmed_devices += 1;
                    if programmed_devices >= max_devices {
                        // log::info!("Done after programming {} devices", max_devices);
                        abort_handle.abort();
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}
