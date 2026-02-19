use std::{
    collections::HashMap,
    io::{self, Write},
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use anyhow::{Context as AnyHowContext, Result};
use rusb::{Context, Device, DeviceHandle, Direction, Result as RusbResult, UsbContext};

const VENDOR_ID: u16 = 0x08f2;
const PRODUCT_ID: u16 = 0x6811;

const BUTTONS_INTERAFCE: u8 = 1;
const TABLET_INTERFACE: u8 = 2;

struct USBDevice<T: UsbContext> {
    pub device: Device<T>,
    pub handle: DeviceHandle<T>,
    pub interfaces: HashMap<u8, InterfaceInfo>,
}

impl<T: UsbContext> Drop for USBDevice<T> {
    fn drop(&mut self) {
        let interfaces: Vec<u8> = self.interfaces.keys().into_iter().map(|v| *v).collect();

        for i in interfaces {
            if let Ok(res) = self.handle.kernel_driver_active(i) {
                if res {
                    let _ = self.handle.attach_kernel_driver(i);
                }
            }
        }
    }
}

#[derive(Hash, Clone)]
pub struct InterfaceInfo {
    pub number: u8,
    pub endpoint: u8,
}

struct MessageDevice {
    pub request_type: u8,
    pub request: u8,
    pub value: u16,
    pub interface: u8,
    pub payload: Vec<u8>,
    pub timeout: Duration,
}

fn main() -> Result<()> {
    let running = Arc::new(AtomicBool::new(true));

    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, std::sync::atomic::Ordering::SeqCst);
    })
    .expect("Unlonw handle error");

    let mut context = Context::new()?;

    let mut usb_device =
        open_device(&mut context, VENDOR_ID, PRODUCT_ID)?.context("Tablet Not Found")?;

    claim_interfaces(
        &mut usb_device.handle,
        &[BUTTONS_INTERAFCE, TABLET_INTERFACE],
    )?;

    let mtm_1106_init = MessageDevice {
        request_type: 0x21,
        request: 0x09,
        value: 0x0302,
        interface: TABLET_INTERFACE as u8,
        payload: vec![0x02, 0x02, 0xb5, 0x02, 0x00, 0x00, 0x00, 0x00],
        timeout: Duration::from_secs(1),
    };

    send_to_device(&mut usb_device.handle, &mtm_1106_init)?;

    while running.load(std::sync::atomic::Ordering::SeqCst) {
        match read_device(
            &mut usb_device.handle,
            usb_device.interfaces.get(&2).unwrap(),
            100,
        ) {
            Ok((id, bytes)) => println!("Interface: {id} || Bytes: {bytes:02X?}"),
            Err(rusb::Error::Timeout) => {
                print!(".");
                io::stdout().flush().unwrap();
            }
            Err(e) => {
                println!("Erro fatal na leitura: {:?}", e);
                break;
            }
        }

        match read_device(
            &mut usb_device.handle,
            usb_device.interfaces.get(&1).unwrap(),
            10,
        ) {
            Ok((id, bytes)) => println!("Interface: {id} || Bytes: {bytes:02X?}"),
            Err(rusb::Error::Timeout) => {
                print!(".");
                io::stdout().flush().unwrap();
            }
            Err(e) => {
                println!("Erro fatal na leitura: {:?}", e);
                break;
            }
        }
    }

    Ok(())
}

fn open_device<T: UsbContext>(context: &mut T, vid: u16, pid: u16) -> Result<Option<USBDevice<T>>> {
    let devices = context.devices()?;

    for device in devices.iter() {
        let desc = device.device_descriptor()?;

        if desc.vendor_id() == vid && desc.product_id() == pid {
            let handle = device.open()?;
            let mut interfaces = HashMap::new();

            let config_descriptor = device.active_config_descriptor()?;
            for int in config_descriptor.interfaces() {
                let number = int.number();
                for desc in int.descriptors() {
                    for endpoint in desc.endpoint_descriptors() {
                        if endpoint.direction() == Direction::In {
                            let endpoint = endpoint.address();
                            interfaces.insert(number, InterfaceInfo { number, endpoint });
                            break;
                        }
                    }
                }
            }

            return Ok(Some(USBDevice {
                device: device,
                handle: handle,
                interfaces,
            }));
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

fn send_to_device<T: UsbContext>(
    handle: &mut DeviceHandle<T>,
    message: &MessageDevice,
) -> Result<()> {
    handle.write_control(
        message.request_type,
        message.request,
        message.value,
        message.interface as u16,
        &message.payload,
        message.timeout,
    )?;

    Ok(())
}

fn read_device<T: UsbContext>(
    handle: &mut DeviceHandle<T>,
    interface: &InterfaceInfo,
    timeout: u64,
) -> RusbResult<(u8, Vec<u8>)> {
    let mut buffer = vec![0; 64];

    let reads = handle.read_interrupt(
        interface.endpoint,
        &mut buffer,
        Duration::from_millis(timeout),
    )?;
    println!("{reads}");
    Ok((interface.number, buffer[..reads].to_vec()))
}
