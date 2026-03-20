use anyhow::{anyhow, Context, Result};
use libc::{c_char, c_short};
use std::ffi::CString;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{FromRawFd, RawFd};
use std::process::Command;
use std::sync::{Arc, Mutex};

const TUNSETIFF: libc::c_ulong = 0x400454ca;
const IFF_TUN: c_short = 0x0001;
const IFF_NO_PI: c_short = 0x1000;

#[repr(C)]
struct IfReq {
    ifr_name: [c_char; libc::IFNAMSIZ],
    ifr_ifru: [u8; 24],
}

#[derive(Clone)]
pub struct TunDevice {
    reader: Arc<Mutex<File>>,
    writer: Arc<Mutex<File>>,
    name: String,
}

impl TunDevice {
    pub fn create(name: &str, address: Option<&str>, mtu: u32) -> Result<Self> {
        let fd = open_tun()?;
        configure_tun(fd, name)?;

        let file = unsafe { File::from_raw_fd(fd) };
        let reader = file
        .try_clone()
        .context("failed to clone TUN fd for reader")?;
        let writer = file;

        let dev = TunDevice {
            reader: Arc::new(Mutex::new(reader)),
            writer: Arc::new(Mutex::new(writer)),
            name: name.to_string(),
        };

        if let Some(addr) = address {
            run_ip(&["addr", "replace", addr, "dev", name])?;
        }
        run_ip(&["link", "set", "dev", name, "mtu", &mtu.to_string()])?;
        run_ip(&["link", "set", "dev", name, "up"])?;

        Ok(dev)
    }

    pub fn read_packet(&self, buf: &mut [u8]) -> Result<usize> {
        let mut guard = self
        .reader
        .lock()
        .map_err(|_| anyhow!("tun reader lock poisoned"))?;

        let n = guard.read(buf).context("failed to read from tun")?;
        Ok(n)
    }

    pub fn write_packet(&self, packet: &[u8]) -> Result<()> {
        let mut guard = self
        .writer
        .lock()
        .map_err(|_| anyhow!("tun writer lock poisoned"))?;

        guard
        .write_all(packet)
        .context("failed to write to tun")?;
        Ok(())
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

fn open_tun() -> Result<RawFd> {
    let path = CString::new("/dev/net/tun").unwrap();
    let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDWR) };
    if fd < 0 {
        return Err(anyhow!("failed to open /dev/net/tun"));
    }
    Ok(fd)
}

fn configure_tun(fd: RawFd, name: &str) -> Result<()> {
    let mut ifr = IfReq {
        ifr_name: [0; libc::IFNAMSIZ],
        ifr_ifru: [0u8; 24],
    };

    for (i, b) in name.bytes().take(libc::IFNAMSIZ - 1).enumerate() {
        ifr.ifr_name[i] = b as c_char;
    }

    let flags = (IFF_TUN | IFF_NO_PI).to_ne_bytes();
    ifr.ifr_ifru[0] = flags[0];
    ifr.ifr_ifru[1] = flags[1];

    let res = unsafe { libc::ioctl(fd, TUNSETIFF, &ifr) };
    if res < 0 {
        return Err(anyhow!("ioctl(TUNSETIFF) failed for interface {name}"));
    }

    Ok(())
}

fn run_ip(args: &[&str]) -> Result<()> {
    let status = Command::new("ip")
    .args(args)
    .status()
    .with_context(|| format!("failed to run `ip {}`", args.join(" ")))?;

    if !status.success() {
        return Err(anyhow!("`ip {}` failed with status {status}", args.join(" ")));
    }

    Ok(())
}
