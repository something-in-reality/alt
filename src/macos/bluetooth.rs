//! Paired Bluetooth devices, through IOBluetooth.

use objc2::rc::Retained;
use objc2_io_bluetooth::IOBluetoothDevice;

use super::Error;
use crate::action::{Bluetooth, Device};

pub fn perform(action: &Bluetooth) -> Result<(), Error> {
    let (device, connect) = match action {
        Bluetooth::Connect(device) => (device, Some(true)),
        Bluetooth::Disconnect(device) => (device, Some(false)),
        Bluetooth::Toggle(device) => (device, None),
    };
    let paired = paired(device)?;
    let connect = connect.unwrap_or_else(|| !unsafe { paired.isConnected() });
    let status = unsafe {
        if connect {
            paired.openConnection()
        } else {
            paired.closeConnection()
        }
    };
    match status {
        0 => Ok(()),
        status => Err(Error::Bluetooth(device.clone(), status)),
    }
}

/// The paired device named `device`, or with its address.
fn paired(device: &Device) -> Result<Retained<IOBluetoothDevice>, Error> {
    let address = |s: &str| s.replace(':', "-").to_ascii_lowercase();
    unsafe { IOBluetoothDevice::pairedDevices() }
        .into_iter()
        .flat_map(|devices| devices.to_vec())
        .filter_map(|paired| paired.downcast::<IOBluetoothDevice>().ok())
        .find(|paired| {
            let (name, found) = unsafe { (paired.name(), paired.addressString()) };
            name.to_string() == device.as_ref()
                || found
                    .is_some_and(|found| address(&found.to_string()) == address(device.as_ref()))
        })
        .ok_or_else(|| Error::NotPaired(device.clone()))
}
