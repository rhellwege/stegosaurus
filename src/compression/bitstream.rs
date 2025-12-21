use std::collections::VecDeque;
use std::io::{Read, Write};

use anyhow::{Context, Result};

pub struct BitStream {
    bytes: VecDeque<u8>,
    wbuf_byte: u8,
    wbuf_index: u8,
    rbuf_byte: u8,
    rbuf_index: u8,
}

impl BitStream {
    pub fn new() -> Self {
        BitStream {
            bytes: VecDeque::new(),
            wbuf_byte: 0,
            wbuf_index: 0,
            rbuf_byte: 0,
            rbuf_index: 0,
        }
    }

    /// value should contain the bits to be written in the lsb side
    /// write 0b1001 => bs.write_n_bits(0b00001001, 4);
    pub fn write_n_bits(&mut self, n: u8, value: u8) {
        assert!(n <= 8, "Cannot write more than 8 bits");
        // fill the buf byte from lsb to msb
        // 0b00001001
        // buf index = 4
        // buf = 0b1001
        let value = value & (0xff >> (8 - n));
        if self.wbuf_index + n > 8 {
            let overflow = self.wbuf_index + n - 8;
            self.wbuf_byte <<= 8 - self.wbuf_index;
            self.wbuf_byte |= value.checked_shr(overflow as u32).unwrap_or(0);
            self.bytes.push_back(self.wbuf_byte);
            self.wbuf_byte = value & (0xff >> (8 - overflow));
            self.wbuf_index = overflow;
        } else {
            self.wbuf_byte = self.wbuf_byte.checked_shl(n as u32).unwrap_or(0);
            self.wbuf_index += n;
            self.wbuf_byte |= value;
        }
    }

    /// n bits will be returned on the lsb side
    /// only the right most n bits will be overwritten.
    /// the requested number of bits requested will be zeroed out
    /// returns the number of bits read
    pub fn read_n_bits(&mut self, n: u8, out_buf: &mut u8) -> Result<usize> {
        assert!(n <= 8, "Cannot read more than 8 bits");
        *out_buf &= (0xff as u8).checked_shl(n as u32).unwrap_or(0);
        // read from the write buffer
        if self.rbuf_index == 0 && self.bytes.is_empty() {
            if n > self.wbuf_index {
                let nread = self.wbuf_index;
                *out_buf |= self.wbuf_byte;
                self.wbuf_index = 0;
                self.wbuf_byte = 0;
                return Ok(nread as usize);
            } else {
                *out_buf |= self.wbuf_byte >> (self.wbuf_index - n);
                self.wbuf_index -= n;
                self.wbuf_byte &= (0xff as u8)
                    .checked_shr(8 - self.wbuf_index as u32)
                    .unwrap_or(0);
                return Ok(n as usize);
            }
        } else {
            // rbuf reads from left to right. << msb to lsb
            // if we exhaust the read buffer, pop a byte if possible
            if n > self.rbuf_index {
                // we need more bits either from bytes or wbuf
                if let Some(next) = self.bytes.pop_front() {
                    // we have all the bits we could need from the next byte
                    let remaining = n - self.rbuf_index; // how many more bits we need
                    *out_buf |= self.rbuf_byte >> (8 - n); // flush the read buffer first
                    *out_buf |= next >> (8 - remaining);
                    self.rbuf_byte = next << remaining;
                    self.rbuf_index = 8 - remaining;
                    return Ok(n as usize);
                } else {
                    // pull bits from the write buffer if possible
                    let to_borrow = n - self.rbuf_index;
                    if self.wbuf_index < to_borrow {
                        // flush the read buffer first
                        let len = self.rbuf_index + self.wbuf_index;
                        *out_buf |= self.rbuf_byte >> (8 - len);
                        self.rbuf_byte = 0;
                        self.rbuf_index = 0;

                        *out_buf |= self.wbuf_byte;
                        self.wbuf_byte = 0;
                        self.wbuf_index = 0;

                        return Ok(len as usize);
                    } else {
                        // flush the read buffer first
                        *out_buf |= self.rbuf_byte >> (8 - n);
                        self.rbuf_byte = 0;
                        self.rbuf_index = 0;
                        *out_buf |= self.wbuf_byte >> (self.wbuf_index - to_borrow);
                        self.wbuf_byte &= (0xff as u8)
                            .checked_shr(8 - (self.wbuf_index - to_borrow) as u32)
                            .unwrap_or(0);
                        self.wbuf_index -= to_borrow;
                        return Ok(n as usize);
                    }
                }
            } else {
                *out_buf |= self.rbuf_byte >> (8 - n);
                self.rbuf_byte <<= n;
                self.rbuf_index -= n;
                return Ok(n as usize);
            }
        }
    }

    /// Flushes the buffer to the output stream.
    /// Only do this at the end of the stream.
    pub fn flush(&mut self) {
        if self.wbuf_index != 0 {
            self.wbuf_byte <<= 8 - self.wbuf_index;
            self.bytes.push_back(self.wbuf_byte);
            self.wbuf_byte = 0;
            self.wbuf_index = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn read_empty() {
        let mut rw = BitStream::new();

        let mut buf: u8 = 0;
        let nread = rw.read_n_bits(1, &mut buf).unwrap();
        assert!(nread == 0);
    }

    #[test]
    fn write_read() {
        let mut rw = BitStream::new();
        let mut buf: u8 = 0;

        // write 0b111
        rw.write_n_bits(3, 0b111);

        let nread = rw.read_n_bits(1, &mut buf).unwrap();
        assert!(nread == 1);
        assert!(buf == 0b00000001);

        let nread = rw.read_n_bits(2, &mut buf).unwrap();
        assert!(nread == 2);
        assert!(buf == 0b00000011);

        let nread = rw.read_n_bits(2, &mut buf).unwrap();
        assert!(nread == 0);
        assert!(buf == 0b00000000);
    }

    #[test]
    fn write_read_bytes() {
        let mut rw = BitStream::new();
        let mut buf: u8 = 0;

        rw.write_n_bits(8, 0b10101010);
        rw.write_n_bits(8, 0b10101010);
        rw.write_n_bits(5, 0b10101);

        // 0b101010101010101010101
        let nread = rw.read_n_bits(1, &mut buf).unwrap();
        assert!(nread == 1);
        assert!(buf == 0b00000001);

        // 0b01010101010101010101
        let nread = rw.read_n_bits(2, &mut buf).unwrap();
        assert!(nread == 2);
        assert!(buf == 0b00000001);

        // 0b010101010101010101
        let nread = rw.read_n_bits(4, &mut buf).unwrap();
        assert!(nread == 4);
        assert!(buf == 0b00000101);

        // 0b01010101010101
        let nread = rw.read_n_bits(8, &mut buf).unwrap();
        assert!(nread == 8);
        assert!(buf == 0b01010101);

        buf = 0;
        // 0b010101
        let nread = rw.read_n_bits(8, &mut buf).unwrap();
        assert!(nread == 6);
        assert!(buf == 0b010101);

        let nread = rw.read_n_bits(8, &mut buf).unwrap();
        assert!(nread == 0);
        assert!(buf == 0b00000000);
    }

    #[test]
    fn rbuf_wbuf() {
        let mut rw = BitStream::new();
        let mut buf: u8 = 0;

        rw.write_n_bits(8, 0b10101010);
        rw.write_n_bits(5, 0b11111);

        // 0b1010101011111
        let nread = rw.read_n_bits(5, &mut buf).unwrap();
        assert!(nread == 5);
        assert!(buf == 0b010101);

        // 0b01011111
        let nread = rw.read_n_bits(8, &mut buf).unwrap();
        assert!(nread == 8);
        assert!(buf == 0b01011111);

        let nread = rw.read_n_bits(8, &mut buf).unwrap();
        assert!(nread == 0);
        assert!(buf == 0b0);
    }
}
