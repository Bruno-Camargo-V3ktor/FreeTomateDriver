use std::{
    collections::HashMap,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use anyhow::{Context as AnyHowContext, Result};
use rusb::{Context, Device, DeviceHandle, Direction, Result as RusbResult, UsbContext};

const VENDOR_ID: u16 = 0x08f2;
const PRODUCT_ID: u16 = 0x6811;

const MASS_STORAGE: u8 = 0;
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
    pub endpoints_in: Vec<u8>,
    pub endpoints_out: Vec<u8>,
}

struct MessageDevice {
    pub request_type: u8,
    pub request: u8,
    pub value: u16,
    pub interface: u16,
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
        &[MASS_STORAGE, BUTTONS_INTERAFCE, TABLET_INTERFACE],
    )?;

    std::thread::sleep(Duration::from_millis(500));

    let magic_packet = MessageDevice {
        request_type: 0x21,
        request: 0x09,
        value: 0x0202,
        interface: 2,
        payload: vec![0x02, 0x01],
        timeout: Duration::from_secs(1),
    };
    let _ = send_to_device(&mut usb_device.handle, &magic_packet);

    std::thread::sleep(Duration::from_millis(500));

    while running.load(std::sync::atomic::Ordering::SeqCst) {
        match read_device(
            &mut usb_device.handle,
            usb_device.interfaces.get(&BUTTONS_INTERAFCE).unwrap(),
            8,
            10,
        ) {
            Ok((id, bytes)) => println!("Interface: {id} || Bytes: {bytes:02X?}"),
            Err(rusb::Error::Timeout) => {
                //print!(".");
                //io::stdout().flush().unwrap();
            }
            Err(e) => {
                println!("Erro fatal na leitura: {:?}", e);
                break;
            }
        }

        match read_device(
            &mut usb_device.handle,
            usb_device.interfaces.get(&TABLET_INTERFACE).unwrap(),
            8,
            10,
        ) {
            Ok((id, bytes)) => println!("Interface: {id} || Bytes: {bytes:02X?}"),
            Err(rusb::Error::Timeout) => {
                //print!(".");
                //io::stdout().flush().unwrap();
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
                    let mut endpoints_in = vec![];
                    let mut endpoints_out = vec![];
                    for endpoint in desc.endpoint_descriptors() {
                        if endpoint.direction() == Direction::In {
                            endpoints_in.push(endpoint.address());
                        }

                        if endpoint.direction() == Direction::Out {
                            endpoints_out.push(endpoint.address());
                        }
                    }
                    interfaces.insert(
                        number,
                        InterfaceInfo {
                            number,
                            endpoints_in,
                            endpoints_out,
                        },
                    );
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
        message.interface,
        &message.payload,
        message.timeout,
    )?;

    Ok(())
}

fn read_device<T: UsbContext>(
    handle: &mut DeviceHandle<T>,
    interface: &InterfaceInfo,
    bytes: usize,
    timeout: u64,
) -> RusbResult<(u8, Vec<u8>)> {
    let mut buffer = vec![0; bytes];
    let mut res = Ok(0);

    for endpoint in &interface.endpoints_in {
        res = handle.read_interrupt(*endpoint, &mut buffer, Duration::from_millis(timeout));

        if let Ok(bytes_read) = &res {
            return Ok((interface.number, buffer[..(*bytes_read)].to_vec()));
        }
    }

    let bytes_read = res?;
    Ok((interface.number, buffer[..bytes_read].to_vec()))
}
