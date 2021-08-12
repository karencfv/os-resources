use core::fmt;
use core::fmt::Write;
use volatile::Volatile;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// The repr(u8) attribute stores each enum variant as a u8
#[repr(u8)]
pub enum Colour {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
struct ColourCode(u8);

impl ColourCode {
    fn new(foreground: Colour, background: Colour) -> ColourCode {
        ColourCode((background as u8) << 4 | (foreground as u8))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// Since field ordering is undefined in Rust, the repr(C) attribute
// guarantees that the fields are laid out exactly as in C
#[repr(C)]
struct ScreenChar {
    ascii_character: u8,
    colour_code: ColourCode,
}

const BUFFER_HEIGHT: usize = 25;
const BUFFER_WIDTH: usize = 80;

// repr(transparent) ensures it has the same memory layout as its single field
#[repr(transparent)]
struct Buffer {
    // The problem is that we only write to the Buffer and never read from it again.
    // The compiler doesn't know that we really access VGA buffer memory (instead of normal RAM)
    // and knows nothing about the side effect that some characters appear on the screen.
    // So it might decide that these writes are unnecessary and can be omitted.
    // To avoid this erroneous optimization, we need to specify these writes as volatile.
    chars: [[Volatile<ScreenChar>; BUFFER_WIDTH]; BUFFER_HEIGHT],
}

pub struct Writer {
    // Column position keeps track of the current position in the last row
    column_position: usize,
    colour_code: ColourCode,
    // We need the VGA buffer reference to be valid the entirety of the program
    buffer: &'static mut Buffer,
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

impl Writer {
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            byte => {
                if self.column_position >= BUFFER_WIDTH {
                    self.new_line();
                }

                let row = BUFFER_HEIGHT - 1;
                let col = self.column_position;
                let colour_code = self.colour_code;

                self.buffer.chars[row][col].write(ScreenChar {
                    ascii_character: byte,
                    colour_code,
                });
                self.column_position += 1;
            }
        }
    }

    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                // printable ASCII byte range or newline
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                // For unprintable bytes not part of printable ASCII range,
                // we print a ■ character
                _ => self.write_byte(0xfe),
            }
        }
    }

    fn new_line(&mut self) {
        for row in 1..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                let character = self.buffer.chars[row][col].read();
                self.buffer.chars[row - 1][col].write(character);
            }
        }
        self.clear_row(BUFFER_HEIGHT - 1);
        self.column_position = 0;
    }

    fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar {
            ascii_character: b' ',
            colour_code: self.colour_code,
        };

        for col in 0..BUFFER_WIDTH {
            self.buffer.chars[row][col].write(blank);
        }
    }
}

pub fn print_something() {
    let mut writer = Writer {
        column_position: 0,
        colour_code: ColourCode::new(Colour::Pink, Colour::Black),
        buffer: unsafe { &mut *(0xb8000 as *mut Buffer) },
    };

    writer.write_byte(b'W');
    writer.write_string("ello ");
    writer.write_string("Wört! \n");
    write!(writer, "Print numbers {} and {}", 42, 1.0 / 3.0).unwrap();
}