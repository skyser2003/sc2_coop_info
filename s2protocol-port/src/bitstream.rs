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
            return self.read_bits_with_order::<true>(bits);
        }

        self.read_bits_with_order::<false>(bits)
    }

    pub(crate) fn read_u8(&mut self) -> Result<u8, DecodeError> {
        if self.next_bits == 0 {
            if self.used >= self.data.len() {
                return Err(DecodeError::Truncated);
            }

            let value = self.data[self.used];
            self.used += 1;
            return Ok(value);
        }

        Ok(self.read_bits(8)? as u8)
    }

    fn read_bits_with_order<const BIG_ENDIAN: bool>(
        &mut self,
        bits: usize,
    ) -> Result<u64, DecodeError> {
        if bits == 1 {
            return self.read_one_bit();
        }

        if self.next_bits == 0
            && let Some(result) = self.read_aligned_fixed_width::<BIG_ENDIAN>(bits)
        {
            return result;
        }

        if self.next_bits == 0 && bits.is_multiple_of(8) {
            let bytes = bits / 8;
            return self.read_aligned_integer::<BIG_ENDIAN>(bytes);
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

            if BIG_ENDIAN {
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

    fn read_one_bit(&mut self) -> Result<u64, DecodeError> {
        if self.next_bits == 0 {
            if self.used >= self.data.len() {
                return Err(DecodeError::Truncated);
            }

            self.next = self.data[self.used];
            self.used += 1;
            self.next_bits = 8;
        }

        let bit = u64::from(self.next & 1);
        self.next >>= 1;
        self.next_bits -= 1;
        if self.next_bits == 0 {
            self.next = 0;
        }
        Ok(bit)
    }

    fn read_aligned_fixed_width<const BIG_ENDIAN: bool>(
        &mut self,
        bits: usize,
    ) -> Option<Result<u64, DecodeError>> {
        match bits {
            8 => Some(self.read_u8().map(u64::from)),
            16 => Some(self.read_aligned_array::<2>().map(|bytes| {
                let value = if BIG_ENDIAN {
                    u16::from_be_bytes(bytes)
                } else {
                    u16::from_le_bytes(bytes)
                };
                u64::from(value)
            })),
            32 => Some(self.read_aligned_array::<4>().map(|bytes| {
                let value = if BIG_ENDIAN {
                    u32::from_be_bytes(bytes)
                } else {
                    u32::from_le_bytes(bytes)
                };
                u64::from(value)
            })),
            _ => None,
        }
    }

    fn read_aligned_integer<const BIG_ENDIAN: bool>(
        &mut self,
        bytes: usize,
    ) -> Result<u64, DecodeError> {
        let raw = self.read_aligned_slice(bytes)?;
        let mut result = 0u64;
        if BIG_ENDIAN {
            for byte in raw {
                result = (result << 8) | u64::from(*byte);
            }
        } else {
            for (index, byte) in raw.iter().enumerate() {
                result |= u64::from(*byte) << (index * 8);
            }
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
