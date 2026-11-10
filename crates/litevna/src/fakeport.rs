//! A serial port that is not there: the test double behind the driver tests.
//!
//! [`FakePort`] implements `serialport::SerialPort` on top of a [`FakeDevice`],
//! a small state machine that sees every byte the driver writes and queues the
//! bytes a real device would answer. That makes the parts of the drivers that
//! used to need hardware testable: the sweep loop with its duplicate and
//! checksum accounting, the shell's echo and prompt framing, chunked scans,
//! screenshots, and what is (not) sent when something fails.
//!
//! The port is a cheap handle (`Clone`): the test keeps one to look at what was
//! written after the driver has taken the other.

use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serialport::{ClearBuffer, DataBits, FlowControl, Parity, SerialPort, StopBits};

/// The device end of a [`FakePort`].
pub(crate) trait FakeDevice: Send {
    /// The host wrote `bytes`; push whatever the device answers onto `rx`.
    fn on_write(&mut self, bytes: &[u8], rx: &mut VecDeque<u8>);
}

struct Inner {
    rx: VecDeque<u8>,
    written: Vec<u8>,
    device: Box<dyn FakeDevice>,
    unplugged: bool,
}

#[derive(Clone)]
pub(crate) struct FakePort {
    inner: Arc<Mutex<Inner>>,
}

impl FakePort {
    pub(crate) fn new(device: impl FakeDevice + 'static) -> Self {
        FakePort {
            inner: Arc::new(Mutex::new(Inner {
                rx: VecDeque::new(),
                written: Vec::new(),
                device: Box::new(device),
                unplugged: false,
            })),
        }
    }

    /// The port as the drivers take it.
    pub(crate) fn boxed(&self) -> Box<dyn SerialPort> {
        Box::new(self.clone())
    }

    /// Everything the driver has written so far.
    pub(crate) fn written(&self) -> Vec<u8> {
        self.inner.lock().unwrap().written.clone()
    }

    pub(crate) fn clear_written(&self) {
        self.inner.lock().unwrap().written.clear();
    }

    /// From now on every read and write fails like a pulled USB cable.
    pub(crate) fn unplug(&self) {
        self.inner.lock().unwrap().unplugged = true;
    }
}

fn gone() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "fake port unplugged")
}

impl io::Read for FakePort {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut inner = self.inner.lock().unwrap();
        if inner.unplugged {
            return Err(gone());
        }
        if inner.rx.is_empty() {
            drop(inner);
            // A real port blocks for its timeout; a short sleep keeps a driver
            // that waits for a deadline from spinning.
            std::thread::sleep(Duration::from_millis(1));
            return Err(io::Error::new(io::ErrorKind::TimedOut, "no data"));
        }
        let n = buf.len().min(inner.rx.len());
        for b in buf.iter_mut().take(n) {
            *b = inner.rx.pop_front().unwrap();
        }
        Ok(n)
    }
}

impl io::Write for FakePort {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut inner = self.inner.lock().unwrap();
        if inner.unplugged {
            return Err(gone());
        }
        inner.written.extend_from_slice(buf);
        let Inner { device, rx, .. } = &mut *inner;
        device.on_write(buf, rx);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl SerialPort for FakePort {
    fn name(&self) -> Option<String> {
        Some("fake".into())
    }
    fn baud_rate(&self) -> serialport::Result<u32> {
        Ok(115_200)
    }
    fn data_bits(&self) -> serialport::Result<DataBits> {
        Ok(DataBits::Eight)
    }
    fn flow_control(&self) -> serialport::Result<FlowControl> {
        Ok(FlowControl::None)
    }
    fn parity(&self) -> serialport::Result<Parity> {
        Ok(Parity::None)
    }
    fn stop_bits(&self) -> serialport::Result<StopBits> {
        Ok(StopBits::One)
    }
    fn timeout(&self) -> Duration {
        Duration::from_millis(1)
    }
    fn set_baud_rate(&mut self, _: u32) -> serialport::Result<()> {
        Ok(())
    }
    fn set_data_bits(&mut self, _: DataBits) -> serialport::Result<()> {
        Ok(())
    }
    fn set_flow_control(&mut self, _: FlowControl) -> serialport::Result<()> {
        Ok(())
    }
    fn set_parity(&mut self, _: Parity) -> serialport::Result<()> {
        Ok(())
    }
    fn set_stop_bits(&mut self, _: StopBits) -> serialport::Result<()> {
        Ok(())
    }
    fn set_timeout(&mut self, _: Duration) -> serialport::Result<()> {
        Ok(())
    }
    fn write_request_to_send(&mut self, _: bool) -> serialport::Result<()> {
        Ok(())
    }
    fn write_data_terminal_ready(&mut self, _: bool) -> serialport::Result<()> {
        Ok(())
    }
    fn read_clear_to_send(&mut self) -> serialport::Result<bool> {
        Ok(true)
    }
    fn read_data_set_ready(&mut self) -> serialport::Result<bool> {
        Ok(true)
    }
    fn read_ring_indicator(&mut self) -> serialport::Result<bool> {
        Ok(false)
    }
    fn read_carrier_detect(&mut self) -> serialport::Result<bool> {
        Ok(true)
    }
    fn bytes_to_read(&self) -> serialport::Result<u32> {
        Ok(self.inner.lock().unwrap().rx.len() as u32)
    }
    fn bytes_to_write(&self) -> serialport::Result<u32> {
        Ok(0)
    }
    /// Discarding the input drops what the device has sent and the host has not read yet.
    fn clear(&self, buffer: ClearBuffer) -> serialport::Result<()> {
        if matches!(buffer, ClearBuffer::Input | ClearBuffer::All) {
            self.inner.lock().unwrap().rx.clear();
        }
        Ok(())
    }
    fn try_clone(&self) -> serialport::Result<Box<dyn SerialPort>> {
        Ok(Box::new(self.clone()))
    }
    fn set_break(&self) -> serialport::Result<()> {
        Ok(())
    }
    fn clear_break(&self) -> serialport::Result<()> {
        Ok(())
    }
}
