use std::io::{self, BufRead, Read, Write};

use vm::compile_source_for_repl_with_locals;
use vm::compiler::{ReplLocalBinding, TypeSchema};

/// Simple binary format for ReplLocalBinding:
///   count(u32 LE) × { name_len(u32 LE) + name(bytes) + mutable(u8) + schema_tag(u8) + optional(u8) }
fn read_u32(reader: &mut &[u8]) -> Option<u32> {
    let (raw, rest) = reader.split_at(4);
    *reader = rest;
    Some(u32::from_le_bytes(raw.try_into().ok()?))
}

fn read_u8(reader: &mut &[u8]) -> Option<u8> {
    let (&b, rest) = reader.split_first()?;
    *reader = rest;
    Some(b)
}

fn decode_bindings(data: &[u8]) -> Vec<ReplLocalBinding> {
    let mut reader = data;
    let count = match read_u32(&mut reader) {
        Some(c) => c as usize,
        None => return vec![],
    };
    let mut bindings = Vec::with_capacity(count);
    for _ in 0..count {
        let name_len = read_u32(&mut reader).unwrap_or(0) as usize;
        let (name_bytes, rest) = reader.split_at(name_len.min(reader.len()));
        reader = rest;
        let name = String::from_utf8_lossy(name_bytes).to_string();
        let mutable = read_u8(&mut reader).unwrap_or(0) != 0;
        let schema = match read_u8(&mut reader) {
            Some(tag) => match tag {
                0 => Some(TypeSchema::Null),
                1 => Some(TypeSchema::Int),
                2 => Some(TypeSchema::Float),
                3 => Some(TypeSchema::Bool),
                4 => Some(TypeSchema::String),
                5 => Some(TypeSchema::Bytes),
                6 => Some(TypeSchema::Array(Box::new(TypeSchema::Unknown))),
                7 => Some(TypeSchema::Map(Box::new(TypeSchema::Unknown))),
                _ => None,
            },
            None => None,
        };
        let optional = read_u8(&mut reader).unwrap_or(0) != 0;
        bindings.push(ReplLocalBinding {
            name,
            mutable,
            schema,
            optional,
        });
    }
    bindings
}

fn encode_bindings(bindings: &[ReplLocalBinding]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(bindings.len() as u32).to_le_bytes());
    for b in bindings {
        let name_bytes = b.name.as_bytes();
        out.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(name_bytes);
        out.push(u8::from(b.mutable));
        match &b.schema {
            Some(TypeSchema::Null) => out.push(0),
            Some(TypeSchema::Int) => out.push(1),
            Some(TypeSchema::Float) => out.push(2),
            Some(TypeSchema::Bool) => out.push(3),
            Some(TypeSchema::String) => out.push(4),
            Some(TypeSchema::Bytes) => out.push(5),
            Some(TypeSchema::Array(_)) => out.push(6),
            Some(TypeSchema::Map(_)) => out.push(7),
            _ => out.push(8),
        }
        out.push(u8::from(b.optional));
    }
    out
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut stdin = io::stdin().lock();
    let mut source_line = String::new();
    stdin.read_line(&mut source_line)?;
    let source = source_line.trim_end();

    let mut bindings_data = Vec::new();
    stdin.read_to_end(&mut bindings_data)?;

    if source.is_empty() {
        eprintln!("empty input");
        std::process::exit(1);
    }

    let predefined = if bindings_data.is_empty() {
        vec![]
    } else {
        decode_bindings(&bindings_data)
    };

    let result = compile_source_for_repl_with_locals(source, &predefined);
    let compiled = match result {
        Ok(c) => c,
        Err(_) if !source.ends_with(';') => {
            let fallback = format!("{source};");
            match compile_source_for_repl_with_locals(&fallback, &predefined) {
                Ok(c) => c,
                Err(err) => {
                    eprintln!("{err:?}");
                    std::process::exit(1);
                }
            }
        }
        Err(err) => {
            eprintln!("{err:?}");
            std::process::exit(1);
        }
    };

    let encoded = vm::encode_program(
        &compiled
            .compiled
            .program
            .with_local_count(compiled.compiled.locals),
    )?;
    let bindings_out = encode_bindings(&compiled.bindings);

    io::stdout().write_all(&(encoded.len() as u32).to_le_bytes())?;
    io::stdout().write_all(&encoded)?;
    io::stdout().write_all(&bindings_out)?;
    Ok(())
}
