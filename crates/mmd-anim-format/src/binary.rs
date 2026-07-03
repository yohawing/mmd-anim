use glam::Vec3A;

use crate::error::ImportError;

/// Shared little-endian byte cursor for PMX/VMD/PMD binary parsers.
pub(crate) struct ByteReader<'a> {
    pub(crate) data: &'a [u8],
    pub(crate) pos: usize,
}

impl<'a> ByteReader<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub(crate) fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub(crate) fn require(&self, n: usize) -> Result<(), ImportError> {
        if self.remaining() >= n {
            Ok(())
        } else {
            Err(ImportError::UnexpectedEof(
                n.saturating_sub(self.remaining()),
            ))
        }
    }

    pub(crate) fn read_bytes(&mut self, n: usize) -> Result<&'a [u8], ImportError> {
        self.require(n)?;
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    pub(crate) fn read_slice(&mut self, n: usize) -> Result<&'a [u8], ImportError> {
        self.read_bytes(n)
    }

    pub(crate) fn skip(&mut self, n: usize) -> Result<(), ImportError> {
        self.require(n)?;
        self.pos += n;
        Ok(())
    }

    pub(crate) fn read_u8(&mut self) -> Result<u8, ImportError> {
        Ok(self.read_bytes(1)?[0])
    }

    pub(crate) fn read_u16_le(&mut self) -> Result<u16, ImportError> {
        let b = self.read_bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub(crate) fn read_u32_le(&mut self) -> Result<u32, ImportError> {
        let b = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(crate) fn read_i32_le(&mut self) -> Result<i32, ImportError> {
        let b = self.read_bytes(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(crate) fn read_f32_le(&mut self) -> Result<f32, ImportError> {
        let b = self.read_bytes(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(crate) fn read_vec3(&mut self) -> Result<Vec3A, ImportError> {
        Ok(Vec3A::new(
            self.read_f32_le()?,
            self.read_f32_le()?,
            self.read_f32_le()?,
        ))
    }

    pub(crate) fn require_record_bytes(
        &self,
        count: usize,
        record_size: usize,
    ) -> Result<(), ImportError> {
        let bytes = count
            .checked_mul(record_size)
            .ok_or(ImportError::SectionOverflow)?;
        self.require(bytes)
    }

    pub(crate) fn peek_u32_at(&self, pos: usize) -> Option<u32> {
        let bytes = self.data.get(pos..pos + 4)?;
        Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
}

pub(crate) fn write_u16_le(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn write_u32_le(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn write_i32_le(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn write_f32_le(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn write_f32_slice_le(out: &mut Vec<u8>, values: &[f32]) {
    for &value in values {
        write_f32_le(out, value);
    }
}

pub(crate) fn write_fixed_bytes(out: &mut Vec<u8>, value: &[u8], len: usize) {
    let copied = value.len().min(len);
    out.extend_from_slice(&value[..copied]);
    out.resize(out.len() + len - copied, 0);
}
