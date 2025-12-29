use super::DataTransform;
use std::collections::VecDeque;
use std::io::{Read, Write};

use anyhow::{Context, Result, anyhow};

const BUFF_SIZE: usize = 1024;

pub struct BitStream {
    src: Option<Box<dyn Read>>,
    bytes: VecDeque<u8>,
    wbuf_byte: u8,
    wbuf_index: u8,
    rbuf_byte: u8,
    rbuf_index: u8,
}

impl BitStream {
    pub fn new() -> Self {
        BitStream {
            src: None,
            bytes: VecDeque::new(),
            wbuf_byte: 0,
            wbuf_index: 0,
            rbuf_byte: 0,
            rbuf_index: 0,
        }
    }

    pub fn bits_in_stream(&self) -> usize {
        self.wbuf_index as usize + self.rbuf_index as usize + (8 * self.bytes.len())
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

    pub fn write_byte(&mut self, byte: u8) {
        self.write_n_bits(8, byte);
    }
    /// value should contain the bits to be written in the lsb side
    /// write 0b1001 => bs.write_n_bits(0b00001001, 4);
    pub fn write_n_bits_u64(&mut self, n: u8, value: u64) {
        assert!(n <= 64, "Cannot write more than 64 bits");

        // stuff into the msb
        let mut value = value << (64 - n);
        let full_bytes = n / 8;
        let leftover = n % 8;
        for i in 0..full_bytes {
            self.write_byte((value >> (64 - 8)) as u8);
            value <<= 8;
        }
        if leftover > 0 {
            self.write_n_bits(leftover, (value >> (64 - leftover)) as u8);
        }
    }

    fn pull_from_src(&mut self) -> Result<usize> {
        if let Some(ref mut src) = self.src {
            let mut buffer: [u8; BUFF_SIZE] = [0; BUFF_SIZE];
            let nread = src.read(&mut buffer)?;
            let nwrite = self.write(&buffer[0..nread])?;
            if nwrite != nread {
                return Err(anyhow!(
                    "write to bitstream failed. Expected to write a different number of bytes."
                ));
            }
            return Ok(nread);
        }
        Ok(0)
    }

    /// n bits will be returned on the lsb side
    /// out_buf will be zeroed out
    /// the requested number of bits requested will be zeroed out
    /// returns the number of bits read
    pub fn read_n_bits(&mut self, n: u8, out_buf: &mut u8) -> Result<usize> {
        assert!(n <= 8, "Cannot read more than 8 bits");
        // request a byte from our source
        if self.bits_in_stream() < n as usize {
            let _ = self.pull_from_src()?;
        }
        *out_buf = 0;
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
                    self.rbuf_byte = next.checked_shl(remaining as u32).unwrap_or(0);
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

    /// acts as read except does not take away from the stream
    /// returns the number of bits that would be returned from the equivalent read call
    /// must be mutable to pull bytes from the parent stream. Does not consume any bits.
    /// zeroes out the out_buf
    pub fn peek_n_bits(&mut self, n: u8, out_buf: &mut u8) -> Result<usize> {
        // self.peek_n_bits_offset(n, out_buf, 0)
        // request a byte from our source
        if self.bits_in_stream() < n as usize {
            let _ = self.pull_from_src()?;
        }
        *out_buf = self.rbuf_byte;
        *out_buf >>= 8 - n;
        if n <= self.rbuf_index {
            return Ok(n as usize);
        }
        if !self.bytes.is_empty() {
            *out_buf |= self.bytes[0] >> (8 - n - self.rbuf_index);
            return Ok(n as usize);
        }
        if self.rbuf_index + self.wbuf_index >= n {
            *out_buf |= self.wbuf_byte >> (n - self.wbuf_index - self.rbuf_index);
            return Ok(n as usize);
        }
        *out_buf >>= n - self.rbuf_index - self.wbuf_index;
        *out_buf |= self.wbuf_byte;
        Ok(self.rbuf_index as usize + self.wbuf_index as usize)
    }

    /// same as peek_n_bits, but skips the offset number of bits
    /// returns the number of bits read if offset number of bit were consumed
    /// will cause reads to pull from the source to the internal buffer exhaustively
    /// zeroes out the output buf
    pub fn peek_n_bits_offset(
        &mut self,
        n: u8,
        out_buf: &mut u8,
        offset_bits: usize,
    ) -> Result<usize> {
        *out_buf = 0;
        assert!(n <= 8, "Cannot peek more than 8 bits");
        // request a bytes until we exhaust the source stream or we have enough to peek at that offset
        while self.bits_in_stream() <= offset_bits + n as usize {
            let nread = self.pull_from_src()?;
            // we have exhausted our source
            if nread == 0 {
                break;
            }
        }
        let total_bits = self.bits_in_stream();
        let bytes_bits = self.bytes.len() * 8;
        // 1. check if range is to the right of the stream
        if offset_bits >= total_bits {
            return Ok(0);
        }
        // 2. read from wbuf, with n too large
        if offset_bits >= self.rbuf_index as usize + bytes_bits
            && (offset_bits + n as usize) >= total_bits
        {
            let start = total_bits - offset_bits; // offset into the wbuf byte from lsb
            *out_buf = self.wbuf_byte;
            *out_buf &= 0xff >> 8 - start;
            return Ok(start);
        }

        // 3. read from last byte, overflowing to wbuf and end out of bounds
        if !self.bytes.is_empty()
            && offset_bits >= self.rbuf_index as usize
            && (offset_bits + n as usize) >= total_bits
        {
            let start_in_byte = (offset_bits - self.rbuf_index as usize) % 8;
            *out_buf = *self.bytes.back().unwrap();
            *out_buf &= 0xff >> start_in_byte;
            *out_buf <<= self.wbuf_index;
            *out_buf |= self.wbuf_byte;
            return Ok((8 - start_in_byte as usize) + self.wbuf_index as usize);
        }

        // 4. read from wbuf, with n fitting inside
        if offset_bits >= self.rbuf_index as usize + bytes_bits {
            let start = total_bits - offset_bits; // offset into the wbuf byte from lsb
            *out_buf = self.wbuf_byte;
            *out_buf &= 0xff >> 8 - start;
            *out_buf >>= start - n as usize;
            return Ok(n as usize);
        }

        let start_in_byte = (offset_bits
            .checked_sub(self.rbuf_index as usize)
            .unwrap_or(0))
            % 8;

        // 5. read from the last byte overflowing to wbuf, n fits
        if !self.bytes.is_empty()
            && offset_bits >= self.rbuf_index as usize
            && (offset_bits + n as usize) >= self.rbuf_index as usize + bytes_bits
        {
            *out_buf = *self.bytes.back().unwrap();
            *out_buf &= 0xff >> start_in_byte;
            *out_buf <<= n as usize - (8 - start_in_byte);
            let right =
                self.wbuf_byte >> self.wbuf_index as usize - (n as usize - (8 - start_in_byte));
            *out_buf |= right;
            return Ok(n as usize);
        }
        // 6. read from in between two bytes
        if !self.bytes.is_empty()
            && offset_bits >= self.rbuf_index as usize
            && n as usize > (8 - start_in_byte)
        {
            let start_idx = (offset_bits - self.rbuf_index as usize) / 8;
            let end_idx = start_idx + 1;
            *out_buf = self.bytes[start_idx];
            *out_buf &= 0xff >> start_in_byte;
            *out_buf <<= n as usize - (8 - start_in_byte);
            let right = self.bytes[end_idx] >> 8 - (n as usize - (8 - start_in_byte));
            *out_buf |= right;
            return Ok(n as usize);
        }
        // 7. read from within a byte
        if !self.bytes.is_empty() && offset_bits >= self.rbuf_index as usize {
            let start_idx = (offset_bits - self.rbuf_index as usize) / 8;
            *out_buf = self.bytes[start_idx];
            *out_buf &= 0xff >> start_in_byte;
            *out_buf >>= (8 - start_in_byte) - n as usize;
            return Ok(n as usize);
        }
        // 8. read from rbuf AND wbuf with n overflowing
        if offset_bits < self.rbuf_index as usize
            && (offset_bits + n as usize) > self.rbuf_index as usize
            && (offset_bits + n as usize) > total_bits
        {
            *out_buf = self.rbuf_byte;
            *out_buf &= 0xff >> offset_bits;
            *out_buf >>= 8 - self.rbuf_index;
            *out_buf <<= self.wbuf_index;
            *out_buf |= self.wbuf_byte;
            return Ok(total_bits - offset_bits);
        }
        // 9. read from rbuf AND wbuf with n fitting
        if offset_bits < self.rbuf_index as usize
            && (offset_bits + n as usize) > self.rbuf_index as usize
        {
            let bits_in_left = self.rbuf_index as usize - offset_bits;
            *out_buf = self.rbuf_byte;
            *out_buf &= 0xff >> offset_bits;
            *out_buf >>= 8 - self.rbuf_index;
            *out_buf <<= n as usize - bits_in_left;
            let right = self.wbuf_byte >> self.wbuf_index as usize - (n as usize - bits_in_left);
            *out_buf |= right;
            return Ok(n as usize);
        }
        // 10. read from rbuf and overflow
        if offset_bits < self.rbuf_index as usize
            && (offset_bits + n as usize) > self.rbuf_index as usize
        {
            *out_buf = self.rbuf_byte;
            *out_buf &= 0xff >> offset_bits;
            *out_buf >>= 8 - offset_bits - self.rbuf_index as usize;
            return Ok(total_bits - offset_bits);
        }
        // 11. read from rbuf alone
        *out_buf = self.rbuf_byte;
        *out_buf &= 0xff >> offset_bits;
        *out_buf >>= 8 - offset_bits - n as usize;
        return Ok(n as usize);
    }

    pub fn peek_byte(&mut self, buf_byte: &mut u8) -> Result<usize> {
        self.peek_n_bits(8, buf_byte)
    }

    pub fn write_bit(&mut self, bit: bool) {
        self.write_n_bits(1, if bit { 1 } else { 0 });
    }

    pub fn read_bit(&mut self) -> Option<bool> {
        let mut bit: u8 = 0;
        let nread = self.read_n_bits(1, &mut bit).unwrap_or(0);
        if nread == 0 { None } else { Some(bit == 1) }
    }

    /// reads into a byte stuffed into the lsb
    pub fn read_byte(&mut self, byte: &mut u8) -> Result<usize> {
        self.read_n_bits(8, byte)
    }

    /// reads n bits into the lsb
    /// zeroes out destination
    pub fn read_n_bits_u64(&mut self, n: u8, dest: &mut u64) -> Result<usize> {
        let mut buf_byte: u8 = 0;
        let mut bits_read = 0;
        assert!(n <= 64, "Cannot request more than 64 bits into a u64");
        *dest = 0;
        let full_bytes = n / 8;
        let leftover = n % 8;
        for i in 0..full_bytes {
            if let Ok(bits) = self.read_byte(&mut buf_byte) {
                bits_read += bits;
                if bits < 8 {
                    *dest >>= 8 - bits;
                }
                *dest |= (buf_byte as u64) << (n - bits_read as u8);
            } else {
                return Err(anyhow!("failed to read a byte from the bitstream"));
            }
        }
        buf_byte = 0;
        if leftover > 0 {
            if let Ok(bits) = self.read_n_bits(leftover, &mut buf_byte) {
                bits_read += bits;
                *dest >>= n - bits_read as u8;
                *dest |= buf_byte as u64;
                return Ok(bits_read);
            } else {
                return Err(anyhow!("Failed to read leftover bits from bitstream"));
            }
        }
        return Ok(bits_read);
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

    pub fn clear(&mut self) {
        self.wbuf_byte = 0;
        self.wbuf_index = 0;
        self.rbuf_byte = 0;
        self.rbuf_index = 0;
        self.bytes.clear();
    }
}

impl Read for BitStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        for i in 0..buf.len() {
            let nread = self
                .read_byte(&mut buf[i])
                .map_err(|_| std::io::Error::other("Failed to read byte from bitstream"))?;
            if nread < 8 && nread > 0 {
                return Ok(i + 1);
            } else if nread == 0 {
                return Ok(i);
            }
        }
        return Ok(buf.len());
    }
}

impl Write for BitStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut written: usize = 0;
        for byte in buf {
            self.write_byte(*byte);
            written += 1;
        }

        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        // self.flush();
        Ok(())
    }
}

impl DataTransform for BitStream {
    fn attach_reader(&mut self, src: Box<dyn Read>) {
        self.src = Some(src);
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
        assert_eq!(nread, 1);
        assert_eq!(buf, 0b00000001);

        let nread = rw.read_n_bits(2, &mut buf).unwrap();
        assert_eq!(nread, 2);
        assert_eq!(buf, 0b00000011);

        let nread = rw.read_n_bits(2, &mut buf).unwrap();
        assert_eq!(nread, 0);
        // assert_eq!(buf, 0b00000000);
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
        assert_eq!(nread, 1);
        assert_eq!(buf, 0b00000001);

        // 0b01010101010101010101
        let nread = rw.read_n_bits(2, &mut buf).unwrap();
        assert_eq!(nread, 2);
        assert_eq!(buf, 0b00000001);

        // 0b010101010101010101
        let nread = rw.read_n_bits(4, &mut buf).unwrap();
        assert_eq!(nread, 4);
        assert_eq!(buf, 0b00000101);

        // 0b01010101010101
        let nread = rw.read_n_bits(8, &mut buf).unwrap();
        assert_eq!(nread, 8);
        assert_eq!(buf, 0b01010101);

        buf = 0;
        // 0b010101
        let nread = rw.read_n_bits(8, &mut buf).unwrap();
        assert_eq!(nread, 6);
        assert_eq!(buf, 0b010101);

        let nread = rw.read_n_bits(8, &mut buf).unwrap();
        assert_eq!(nread, 0);
        assert_eq!(buf, 0b00000000);
    }

    #[test]
    fn u64_rw() {
        let mut rw = BitStream::new();
        let mut buf: u64 = 0;

        rw.write_n_bits_u64(22, 0b0000001111011010);
        assert_eq!(rw.bits_in_stream(), 22);
        rw.write_n_bits_u64(7, 0b1110111);
        assert_eq!(rw.bits_in_stream(), 22 + 7);

        let nread = rw.read_n_bits_u64(22, &mut buf).unwrap();
        assert_eq!(nread, 22);
        assert_eq!(rw.bits_in_stream(), 7);
        assert_eq!(buf, 0b0000001111011010);

        let nread = rw.read_n_bits_u64(10, &mut buf).unwrap();
        assert_eq!(nread, 7);
        assert_eq!(rw.bits_in_stream(), 0);
        assert_eq!(buf, 0b1110111);

        rw.write_n_bits_u64(19, 0b0000000001111011010);
        assert_eq!(rw.bits_in_stream(), 19);

        rw.write_byte(0xff);

        let nread = rw.read_n_bits_u64(19, &mut buf).unwrap();
        assert_eq!(nread, 19);
        assert_eq!(rw.bits_in_stream(), 8);
        assert_eq!(buf, 0b0000000001111011010);
    }

    #[test]
    fn rbuf_wbuf() {
        let mut rw = BitStream::new();
        let mut buf: u8 = 0;

        rw.write_n_bits(8, 0b10101010);
        rw.write_n_bits(5, 0b11111);

        // 0b1010101011111
        let nread = rw.read_n_bits(5, &mut buf).unwrap();
        assert_eq!(nread, 5);
        assert_eq!(buf, 0b010101);

        // 0b01011111
        let nread = rw.read_n_bits(8, &mut buf).unwrap();
        assert_eq!(nread, 8);
        assert_eq!(buf, 0b01011111);

        let nread = rw.read_n_bits(8, &mut buf).unwrap();
        assert_eq!(nread, 0);
        // assert_eq!(buf, 0b0);
    }

    #[test]
    fn mixed_rw() {
        let test_src = b"123456789abcdefghijklmnopqrstuvwxyz";

        let mut rw = BitStream::new();
        let mut buf: u64 = 0;
        let mut out_buf = [0u8; 256];

        rw.write_n_bits_u64(22, 0b0000001111011010);
        assert_eq!(rw.bits_in_stream(), 22);

        rw.write_n_bits_u64(7, 0b1110111);
        assert_eq!(rw.bits_in_stream(), 22 + 7);

        let nwritten = rw.write(test_src).unwrap();
        assert_eq!(nwritten, test_src.len());
        assert_eq!(rw.bits_in_stream(), 22 + 7 + test_src.len() * 8);

        let nread = rw.read_n_bits_u64(22, &mut buf).unwrap();
        assert_eq!(nread, 22);
        assert_eq!(rw.bits_in_stream(), 7 + test_src.len() * 8);
        assert_eq!(buf, 0b0000001111011010);

        let nread = rw.read_n_bits_u64(7, &mut buf).unwrap();
        assert_eq!(nread, 7);
        assert_eq!(rw.bits_in_stream(), test_src.len() * 8);
        assert_eq!(buf, 0b1110111);

        let nread = rw.read(&mut out_buf).unwrap();
        assert_eq!(nread, test_src.len());
        assert_eq!(&out_buf[0..nread], test_src.as_slice());
    }

    #[test]
    pub fn peek() {
        let mut rw = BitStream::new();
        let mut buf: u64 = 0;
        let mut out_buf = 0u8;

        rw.write_n_bits_u64(12, 0b110111011110);
        let nread = rw.peek_n_bits(2, &mut out_buf).unwrap();
        assert_eq!(nread, 2);
        assert_eq!(out_buf, 0b11);
        assert_eq!(rw.bits_in_stream(), 12);

        let nread = rw.peek_n_bits(3, &mut out_buf).unwrap();
        assert_eq!(nread, 3);
        assert_eq!(out_buf, 0b110);
        assert_eq!(rw.bits_in_stream(), 12);

        let nread = rw.read_n_bits(2, &mut out_buf).unwrap();
        assert_eq!(nread, 2);
        assert_eq!(out_buf, 0b11);
        assert_eq!(rw.bits_in_stream(), 10);

        let nread = rw.peek_n_bits(5, &mut out_buf).unwrap();
        assert_eq!(nread, 5);
        assert_eq!(out_buf, 0b01110);
        assert_eq!(rw.bits_in_stream(), 10);

        let nread = rw.read_n_bits(5, &mut out_buf).unwrap();
        assert_eq!(nread, 5);
        assert_eq!(out_buf, 0b01110);
        assert_eq!(rw.bits_in_stream(), 5);

        let nread = rw.peek_n_bits(5, &mut out_buf).unwrap();
        assert_eq!(nread, 5);
        assert_eq!(out_buf, 0b11110);
        assert_eq!(rw.bits_in_stream(), 5);

        let nread = rw.peek_n_bits(7, &mut out_buf).unwrap();
        assert_eq!(nread, 5);
        assert_eq!(out_buf, 0b11110);
        assert_eq!(rw.bits_in_stream(), 5);

        let b = rw.read_bit().unwrap();
        assert!(b);
        assert_eq!(rw.bits_in_stream(), 4);

        let nread = rw.peek_n_bits(7, &mut out_buf).unwrap();
        assert_eq!(nread, 4);
        assert_eq!(out_buf, 0b1110);
        assert_eq!(rw.bits_in_stream(), 4);
    }

    #[test]
    fn peek_bits_offset() {
        let mut rw = BitStream::new();
        let mut out_buf = 0u8;
        // 00000000 00000000
        rw.write_n_bits_u64(28, 0b000_10011_00110110_10111011_1010);

        let nread = rw.read_n_bits(3, &mut out_buf).unwrap();
        assert_eq!(nread, 3);
        assert_eq!(out_buf, 0b000);

        // 10011_00110110_10111011_1010
        //                              -----
        // 1. offset out of bounds
        let npeek = rw.peek_n_bits_offset(5, &mut out_buf, 25).unwrap();
        assert_eq!(npeek, 0);
        assert_eq!(out_buf, 0);
        // 10011_00110110_10111011_1010
        //                          -----
        // 2. wbuf + overflow
        let npeek = rw.peek_n_bits_offset(5, &mut out_buf, 22).unwrap();
        assert_eq!(npeek, 3);
        assert_eq!(out_buf, 0b010);

        // 10011_00110110_10111011_1010
        //                          --
        // 3. wbuf
        let npeek = rw.peek_n_bits_offset(2, &mut out_buf, 22).unwrap();
        assert_eq!(npeek, 2);
        assert_eq!(out_buf, 0b01);
        // ...
        let npeek = rw.peek_n_bits_offset(4, &mut out_buf, 21).unwrap();
        assert_eq!(npeek, 4);
        assert_eq!(out_buf, 0b1010);

        // 10011_00110110_10111011_1010
        //                      -- ------
        // 4. last byte + wbuf + overflow
        let npeek = rw.peek_n_bits_offset(8, &mut out_buf, 19).unwrap();
        assert_eq!(npeek, 6);
        assert_eq!(out_buf, 0b111010);

        // 10011_00110110_10111011_1010
        //                      -- ---
        // 5. last byte + wbuf
        let npeek = rw.peek_n_bits_offset(5, &mut out_buf, 19).unwrap();
        assert_eq!(npeek, 5);
        assert_eq!(out_buf, 0b11101);
        // ...
        let npeek = rw.peek_n_bits_offset(4, &mut out_buf, 20).unwrap();
        assert_eq!(npeek, 4);
        assert_eq!(out_buf, 0b1101);
        // ...
        let npeek = rw.peek_n_bits_offset(6, &mut out_buf, 19).unwrap();
        assert_eq!(npeek, 6);
        assert_eq!(out_buf, 0b111010);

        // 10011_00110110_10111011_1010
        //             -- --
        // 6. between two bytes
        let npeek = rw.peek_n_bits_offset(4, &mut out_buf, 11).unwrap();
        assert_eq!(npeek, 4);
        assert_eq!(out_buf, 0b1010);
        // ...
        let npeek = rw.peek_n_bits_offset(6, &mut out_buf, 10).unwrap();
        assert_eq!(npeek, 6);
        assert_eq!(out_buf, 0b110101);

        // 10011_00110110_10111011_1010
        //         ----
        // 7. within a byte
        let npeek = rw.peek_n_bits_offset(4, &mut out_buf, 7).unwrap();
        assert_eq!(npeek, 4);
        assert_eq!(out_buf, 0b1101);
        // ...
        let npeek = rw.peek_n_bits_offset(6, &mut out_buf, 14).unwrap();
        assert_eq!(npeek, 6);
        assert_eq!(out_buf, 0b011101);
        // ...
        let npeek = rw.peek_n_bits_offset(8, &mut out_buf, 5).unwrap();
        assert_eq!(npeek, 8);
        assert_eq!(out_buf, 0b00110110);
        // ...
        let npeek = rw.peek_n_bits_offset(8, &mut out_buf, 13).unwrap();
        assert_eq!(npeek, 8);
        assert_eq!(out_buf, 0b10111011);

        // setup for next tests (empty bytes)
        // 111011_1010
        let mut throwaway = 0u64;
        let nread = rw.read_n_bits_u64(15, &mut throwaway).unwrap();
        assert_eq!(nread, 15);
        assert_eq!(throwaway, 0b10011_00110110_10);
        assert_eq!(rw.bits_in_stream(), 10);

        // 111011_1010
        //    --- -----
        // 8. rbuf + wbuf + overflow
        let npeek = rw.peek_n_bits_offset(8, &mut out_buf, 3).unwrap();
        assert_eq!(npeek, 7);
        assert_eq!(out_buf, 0b0111010);
        // ...
        let npeek = rw.peek_n_bits_offset(7, &mut out_buf, 4).unwrap();
        assert_eq!(npeek, 6);
        assert_eq!(out_buf, 0b111010);

        // 111011_1010
        //   ---- --
        // 9. rbuf + wbuf
        let npeek = rw.peek_n_bits_offset(6, &mut out_buf, 2).unwrap();
        assert_eq!(npeek, 6);
        assert_eq!(out_buf, 0b101110);
        // ...
        let npeek = rw.peek_n_bits_offset(6, &mut out_buf, 4).unwrap();
        assert_eq!(npeek, 6);
        assert_eq!(out_buf, 0b111010);
        // ...
        let npeek = rw.peek_n_bits_offset(8, &mut out_buf, 0).unwrap();
        assert_eq!(npeek, 8);
        assert_eq!(out_buf, 0b11101110);
        // ...
        let npeek = rw.peek_n_bits_offset(8, &mut out_buf, 1).unwrap();
        assert_eq!(npeek, 8);
        assert_eq!(out_buf, 0b11011101);

        // setup so that only rbuf has bits
        // might be an impossible state
    }
}
