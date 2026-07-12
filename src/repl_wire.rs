use alloc::string::String;
use alloc::vec::Vec;

const STATE_MAGIC: &[u8; 4] = b"RSR1";
const RESPONSE_MAGIC: &[u8; 4] = b"RSO1";
const MAX_NESTING: usize = 32;

#[derive(Clone, Debug, PartialEq)]
pub enum ReplValue {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Bytes(Vec<u8>),
    Array(Vec<ReplValue>),
    Map(Vec<(ReplValue, ReplValue)>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReplResponse {
    pub locals: Vec<ReplValue>,
    pub result: Option<ReplValue>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplWireError {
    InvalidMagic,
    InvalidTag(u8),
    InvalidBool(u8),
    InvalidUtf8,
    UnexpectedEnd,
    TrailingData,
    LengthOverflow,
    NestingTooDeep,
}

pub fn encode_repl_state(locals: &[ReplValue]) -> Result<Vec<u8>, ReplWireError> {
    let mut output = Vec::new();
    output.extend_from_slice(STATE_MAGIC);
    write_len(&mut output, locals.len())?;
    for value in locals {
        encode_value(value, &mut output, 0)?;
    }
    Ok(output)
}

pub fn decode_repl_state(bytes: &[u8]) -> Result<Vec<ReplValue>, ReplWireError> {
    let mut decoder = Decoder::new(bytes);
    decoder.expect_magic(STATE_MAGIC)?;
    let count = decoder.read_len()?;
    let mut locals = Vec::new();
    locals
        .try_reserve_exact(count)
        .map_err(|_| ReplWireError::LengthOverflow)?;
    for _ in 0..count {
        locals.push(decoder.decode_value(0)?);
    }
    decoder.finish()?;
    Ok(locals)
}

pub fn encode_repl_response(response: &ReplResponse) -> Result<Vec<u8>, ReplWireError> {
    let mut output = Vec::new();
    output.extend_from_slice(RESPONSE_MAGIC);
    write_len(&mut output, response.locals.len())?;
    for value in &response.locals {
        encode_value(value, &mut output, 0)?;
    }
    match &response.result {
        Some(value) => {
            output.push(1);
            encode_value(value, &mut output, 0)?;
        }
        None => output.push(0),
    }
    Ok(output)
}

pub fn decode_repl_response(bytes: &[u8]) -> Result<ReplResponse, ReplWireError> {
    let mut decoder = Decoder::new(bytes);
    decoder.expect_magic(RESPONSE_MAGIC)?;
    let count = decoder.read_len()?;
    let mut locals = Vec::new();
    locals
        .try_reserve_exact(count)
        .map_err(|_| ReplWireError::LengthOverflow)?;
    for _ in 0..count {
        locals.push(decoder.decode_value(0)?);
    }
    let result = match decoder.read_u8()? {
        0 => None,
        1 => Some(decoder.decode_value(0)?),
        value => return Err(ReplWireError::InvalidBool(value)),
    };
    decoder.finish()?;
    Ok(ReplResponse { locals, result })
}

fn write_len(output: &mut Vec<u8>, length: usize) -> Result<(), ReplWireError> {
    let length = u32::try_from(length).map_err(|_| ReplWireError::LengthOverflow)?;
    output.extend_from_slice(&length.to_le_bytes());
    Ok(())
}

fn encode_value(
    value: &ReplValue,
    output: &mut Vec<u8>,
    depth: usize,
) -> Result<(), ReplWireError> {
    if depth > MAX_NESTING {
        return Err(ReplWireError::NestingTooDeep);
    }
    match value {
        ReplValue::Null => output.push(0),
        ReplValue::Int(value) => {
            output.push(1);
            output.extend_from_slice(&value.to_le_bytes());
        }
        ReplValue::Float(value) => {
            output.push(2);
            output.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        ReplValue::Bool(value) => {
            output.push(3);
            output.push(u8::from(*value));
        }
        ReplValue::String(value) => {
            output.push(4);
            write_len(output, value.len())?;
            output.extend_from_slice(value.as_bytes());
        }
        ReplValue::Bytes(value) => {
            output.push(5);
            write_len(output, value.len())?;
            output.extend_from_slice(value);
        }
        ReplValue::Array(values) => {
            output.push(6);
            write_len(output, values.len())?;
            for value in values {
                encode_value(value, output, depth + 1)?;
            }
        }
        ReplValue::Map(entries) => {
            output.push(7);
            write_len(output, entries.len())?;
            for (key, value) in entries {
                encode_value(key, output, depth + 1)?;
                encode_value(value, output, depth + 1)?;
            }
        }
    }
    Ok(())
}

struct Decoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Decoder<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn finish(self) -> Result<(), ReplWireError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(ReplWireError::TrailingData)
        }
    }

    fn expect_magic(&mut self, expected: &[u8; 4]) -> Result<(), ReplWireError> {
        if self.read_exact(expected.len())? == expected {
            Ok(())
        } else {
            Err(ReplWireError::InvalidMagic)
        }
    }

    fn read_u8(&mut self) -> Result<u8, ReplWireError> {
        Ok(self.read_exact(1)?[0])
    }

    fn read_u32(&mut self) -> Result<u32, ReplWireError> {
        let bytes: [u8; 4] = self
            .read_exact(4)?
            .try_into()
            .map_err(|_| ReplWireError::UnexpectedEnd)?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn read_len(&mut self) -> Result<usize, ReplWireError> {
        usize::try_from(self.read_u32()?).map_err(|_| ReplWireError::LengthOverflow)
    }

    fn read_exact(&mut self, length: usize) -> Result<&'a [u8], ReplWireError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(ReplWireError::LengthOverflow)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(ReplWireError::UnexpectedEnd)?;
        self.offset = end;
        Ok(value)
    }

    fn decode_value(&mut self, depth: usize) -> Result<ReplValue, ReplWireError> {
        if depth > MAX_NESTING {
            return Err(ReplWireError::NestingTooDeep);
        }
        match self.read_u8()? {
            0 => Ok(ReplValue::Null),
            1 => {
                let bytes: [u8; 8] = self
                    .read_exact(8)?
                    .try_into()
                    .map_err(|_| ReplWireError::UnexpectedEnd)?;
                Ok(ReplValue::Int(i64::from_le_bytes(bytes)))
            }
            2 => {
                let bytes: [u8; 8] = self
                    .read_exact(8)?
                    .try_into()
                    .map_err(|_| ReplWireError::UnexpectedEnd)?;
                Ok(ReplValue::Float(f64::from_bits(u64::from_le_bytes(bytes))))
            }
            3 => match self.read_u8()? {
                0 => Ok(ReplValue::Bool(false)),
                1 => Ok(ReplValue::Bool(true)),
                value => Err(ReplWireError::InvalidBool(value)),
            },
            4 => {
                let length = self.read_len()?;
                let bytes = self.read_exact(length)?;
                let value = core::str::from_utf8(bytes).map_err(|_| ReplWireError::InvalidUtf8)?;
                Ok(ReplValue::String(String::from(value)))
            }
            5 => {
                let length = self.read_len()?;
                Ok(ReplValue::Bytes(self.read_exact(length)?.to_vec()))
            }
            6 => {
                let count = self.read_len()?;
                let mut values = Vec::new();
                values
                    .try_reserve_exact(count)
                    .map_err(|_| ReplWireError::LengthOverflow)?;
                for _ in 0..count {
                    values.push(self.decode_value(depth + 1)?);
                }
                Ok(ReplValue::Array(values))
            }
            7 => {
                let count = self.read_len()?;
                let mut entries = Vec::new();
                entries
                    .try_reserve_exact(count)
                    .map_err(|_| ReplWireError::LengthOverflow)?;
                for _ in 0..count {
                    let key = self.decode_value(depth + 1)?;
                    let value = self.decode_value(depth + 1)?;
                    entries.push((key, value));
                }
                Ok(ReplValue::Map(entries))
            }
            tag => Err(ReplWireError::InvalidTag(tag)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repl_state_round_trips_nested_values() {
        let values = vec![
            ReplValue::Null,
            ReplValue::Int(-42),
            ReplValue::Float(2.5),
            ReplValue::Bool(true),
            ReplValue::String(String::from("rss")),
            ReplValue::Bytes(vec![0, 1, 255]),
            ReplValue::Array(vec![ReplValue::Int(1), ReplValue::Bool(false)]),
            ReplValue::Map(vec![(
                ReplValue::String(String::from("answer")),
                ReplValue::Int(42),
            )]),
        ];
        let encoded = encode_repl_state(&values).expect("state should encode");
        assert_eq!(decode_repl_state(&encoded), Ok(values));
    }

    #[test]
    fn repl_response_round_trips_result_and_locals() {
        let response = ReplResponse {
            locals: vec![ReplValue::Int(7)],
            result: Some(ReplValue::String(String::from("done"))),
        };
        let encoded = encode_repl_response(&response).expect("response should encode");
        assert_eq!(decode_repl_response(&encoded), Ok(response));
    }

    #[test]
    fn decoder_rejects_truncated_and_trailing_payloads() {
        let encoded = encode_repl_state(&[ReplValue::Int(1)]).expect("state should encode");
        assert_eq!(
            decode_repl_state(&encoded[..encoded.len() - 1]),
            Err(ReplWireError::UnexpectedEnd)
        );
        let mut trailing = encoded;
        trailing.push(0);
        assert_eq!(
            decode_repl_state(&trailing),
            Err(ReplWireError::TrailingData)
        );
    }
}
