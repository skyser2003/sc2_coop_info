use crate::error::DecodeError;

pub struct BitPackedBuffer {
    data: Vec<u8>,
    used: usize,
    next: u8,
    next_bits: u8,
    big_endian: bool,
}

impl BitPackedBuffer {
    pub fn new(contents: &[u8], big_endian: bool) -> Self {
        BitPackedBuffer {
            data: contents.to_vec(),
            used: 0,
            next: 0,
            next_bits: 0,
            big_endian,
        }
    }

    pub fn done(&self) -> bool {
        self.next_bits == 0 && self.used >= self.data.len()
    }

    pub fn used_bits(&self) -> usize {
        self.used * 8 - self.next_bits as usize
    }

    pub fn byte_align(&mut self) {
        self.next_bits = 0;
    }

    pub fn read_aligned_bytes(&mut self, bytes: usize) -> Result<Vec<u8>, DecodeError> {
        self.byte_align();
        let end = self.used + bytes;
        if end > self.data.len() {
            return Err(DecodeError::Truncated);
        }

        let out = self.data[self.used..end].to_vec();
        self.used = end;
        Ok(out)
    }

    pub fn read_bits(&mut self, bits: usize) -> Result<u64, DecodeError> {
        if bits == 0 {
            return Ok(0);
        }

        if bits > 64 {
            return Err(DecodeError::Corrupted(
                "bit read request exceeds supported width".into(),
            ));
        }

        let mut result: u64 = 0;
        let mut result_bits: usize = 0;

        while result_bits != bits {
            if self.next_bits == 0 {
                if self.used >= self.data.len() {
                    return Err(DecodeError::Truncated);
                }

                self.next = self.data[self.used];
                self.used += 1;
                self.next_bits = 8;
            }

            let bits_remaining = bits - result_bits;
            let copybits = bits_remaining.min(self.next_bits as usize) as u8;
            let mask = if copybits == 8 {
                u8::MAX
            } else {
                ((1u16 << copybits) - 1) as u8
            };
            let copy = (self.next & mask) as u64;

            if self.big_endian {
                result |= copy << (bits - result_bits - copybits as usize);
            } else {
                result |= copy << result_bits;
            }

            if copybits == 8 {
                self.next = 0;
                self.next_bits = 0;
            } else {
                self.next >>= copybits;
                self.next_bits -= copybits;
            }
            result_bits += copybits as usize;
        }

        Ok(result)
    }

    pub fn read_unaligned_bytes(&mut self, bytes: usize) -> Result<Vec<u8>, DecodeError> {
        let mut out = Vec::with_capacity(bytes);
        for _ in 0..bytes {
            out.push(self.read_bits(8)? as u8);
        }
        Ok(out)
    }
}
