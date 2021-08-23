use core::fmt::{Arguments, Write};
use lazy_static::lazy_static;
use spin::Mutex;
use uart_16550::SerialPort;
use x86_64::instructions::interrupts;

// To see the test output on the console, data needs to be sent from our kernel to the host system.
// The data could be sent over a TCP network interface. However, setting up a networking stack is a quite complex task,
// so we will choose a simpler solution instead. A simple way to send the data is to use the serial port,
// an old interface standard (UART) which is no longer found in modern computers.
// It is easy to program and QEMU can redirect the bytes sent over serial to the host's standard output or a file.
lazy_static! {
    pub static ref SERIAL1: Mutex<SerialPort> = {
        // Passing port address 0x3F8, which is the standard port number for the first serial interface.
        let mut serial_port = unsafe { SerialPort::new(0x3F8) };
        serial_port.init();
        Mutex::new(serial_port)
    };
}

#[doc(hidden)]
pub fn _print(args: Arguments) {
    // The without_interrupts function takes a closure and executes it in an interrupt-free environment.
    // We use it to ensure that no interrupt can occur as long as the Mutex is locked.
    interrupts::without_interrupts(|| {
        SERIAL1
            .lock()
            .write_fmt(args)
            .expect("Printing to serial failed");
    });
}

/// Prints to the host through the serial interface
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::serial::_print(format_args!($($arg)*));
    };
}

/// Prints to the host through the serial interface, appending a newline
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($fmt:expr) => ($crate::serial_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::serial_print!(concat!($fmt, "\n"), $($arg)*));
}
