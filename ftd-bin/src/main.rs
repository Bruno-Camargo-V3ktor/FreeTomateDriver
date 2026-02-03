use anyhow::{Context as AnyHowContext, Result};
use rusb::{Context, Device, DeviceHandle, UsbContext};

const VENDOR_ID: u16 = 0x08f2;
const PRODUCT_ID: u16 = 0x6811;

const BUTTONS_INTERAFCE: u8 = 1;
const TABLET_INTERFACE: u8 = 2;

fn main() -> Result<()> {
    let mut context = Context::new()?;

    let (_device, mut handle) =
        open_device(&mut context, VENDOR_ID, PRODUCT_ID)?.context("Tablet Not Found")?;

    claim_interfaces(&mut handle, &[BUTTONS_INTERAFCE, TABLET_INTERFACE])?;

    Ok(())
}

fn open_device<T: UsbContext>(
    context: &mut T,
    vid: u16,
    pid: u16,
) -> Result<Option<(Device<T>, DeviceHandle<T>)>> {
    let devices = context.devices()?;

    for device in devices.iter() {
        let desc = device.device_descriptor()?;

        if desc.vendor_id() == vid && desc.product_id() == pid {
            let handle = device.open()?;
            return Ok(Some((device, handle)));
        }
    }

    Ok(None)
}

fn claim_interfaces<T: UsbContext>(handle: &mut DeviceHandle<T>, interfaces: &[u8]) -> Result<()> {
    for num in interfaces {
        if handle.kernel_driver_active(*num)? {
            handle.detach_kernel_driver(*num)?;
        }
        handle.claim_interface(*num)?;
    }

    Ok(())
}
