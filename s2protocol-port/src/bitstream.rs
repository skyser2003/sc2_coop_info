use crate::error::DecodeError;
use std::convert::TryInto;

pub(crate) struct BitPackedBuffer<'a> {
    data: &'a [u8],
    used: usize,
    next: u8,
    next_bits: u8,
    big_endian: bool,
}

impl<'a> BitPackedBuffer<'a> {
    pub(crate) fn new(contents: &'a [u8], big_endian: bool) -> Self {
        BitPackedBuffer {
            data: contents,
            used: 0,
            next: 0,
            next_bits: 0,
            big_endian,
        }
    }

    pub(crate) fn done(&self) -> bool {
        self.next_bits == 0 && self.used >= self.data.len()
    }

    pub(crate) fn used_bits(&self) -> usize {
        self.used * 8 - self.next_bits as usize
    }

    pub(crate) fn byte_align(&mut self) {
        self.next_bits = 0;
    }

    pub(crate) fn read_aligned_slice(&mut self, bytes: usize) -> Result<&'a [u8], DecodeError> {
        self.byte_align();
        let end = self.used.checked_add(bytes).ok_or(DecodeError::Truncated)?;
        if end > self.data.len() {
            return Err(DecodeError::Truncated);
        }

        let out = &self.data[self.used..end];
        self.used = end;
        Ok(out)
    }

    pub(crate) fn read_aligned_bytes(&mut self, bytes: usize) -> Result<Vec<u8>, DecodeError> {
        Ok(self.read_aligned_slice(bytes)?.to_vec())
    }

    pub(crate) fn read_aligned_array<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let slice = self.read_aligned_slice(N)?;
        slice.try_into().map_err(|_| DecodeError::Truncated)
    }

    pub(crate) fn skip_aligned_bytes(&mut self, bytes: usize) -> Result<(), DecodeError> {
        self.read_aligned_slice(bytes).map(|_| ())
    }

    pub(crate) fn read_bits(&mut self, bits: usize) -> Result<u64, DecodeError> {
        if bits == 0 {
            return Ok(0);
        }

        if bits > 64 {
            return Err(DecodeError::Corrupted(
                "bit read request exceeds supported width".into(),
            ));
        }

        if self.big_endian {
            return self.read_bits_big_endian(bits);
        }

        self.read_bits_little_endian(bits)
    }

    fn read_bits_big_endian(&mut self, bits: usize) -> Result<u64, DecodeError> {
        if self.next_bits == 0 && bits.is_multiple_of(8) {
            let bytes = bits / 8;
            let raw = self.read_aligned_slice(bytes)?;
            let mut result = 0u64;
            for byte in raw {
                result = (result << 8) | u64::from(*byte);
            }
            return Ok(result);
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

            result |= copy << (bits - result_bits - copybits as usize);

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

    fn read_bits_little_endian(&mut self, bits: usize) -> Result<u64, DecodeError> {
        if self.next_bits == 0 && bits.is_multiple_of(8) {
            let bytes = bits / 8;
            let raw = self.read_aligned_slice(bytes)?;
            let mut result = 0u64;
            for (index, byte) in raw.iter().enumerate() {
                result |= u64::from(*byte) << (index * 8);
            }
            return Ok(result);
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

            result |= copy << result_bits;

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

    pub(crate) fn skip_bits(&mut self, bits: usize) -> Result<(), DecodeError> {
        let mut remaining = bits;

        if self.next_bits > 0 {
            let copybits = remaining.min(self.next_bits as usize) as u8;
            if copybits == self.next_bits {
                self.next = 0;
                self.next_bits = 0;
            } else {
                self.next >>= copybits;
                self.next_bits -= copybits;
            }
            remaining -= copybits as usize;
        }

        let aligned_bytes = remaining / 8;
        if aligned_bytes > 0 {
            self.skip_aligned_bytes(aligned_bytes)?;
            remaining -= aligned_bytes * 8;
        }

        if remaining > 0 {
            self.read_bits(remaining)?;
        }

        Ok(())
    }

    pub(crate) fn read_unaligned_array<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        if self.next_bits == 0 {
            return self.read_aligned_array();
        }

        let mut out = [0u8; N];
        for byte in &mut out {
            *byte = self.read_bits(8)? as u8;
        }
        Ok(out)
    }

    pub(crate) fn skip_unaligned_bytes(&mut self, bytes: usize) -> Result<(), DecodeError> {
        let bits = bytes
            .checked_mul(8)
            .ok_or_else(|| DecodeError::Corrupted("byte skip width overflows usize".into()))?;
        self.skip_bits(bits)
    }
}
