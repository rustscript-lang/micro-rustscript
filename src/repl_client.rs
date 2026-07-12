use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{self, Read, Write};

use vm::compiler::TypeSchema;
use vm::{OpCode, ReplLocalBinding, ReplLocalState, SourceError, ValueType};

use crate::{ReplResponse, ReplValue, ReplWireError, decode_repl_response, encode_repl_state};

pub const REPL_REQUEST_MAGIC: [u8; 4] = *b"RSSQ";
pub const REPL_RESPONSE_MAGIC: [u8; 4] = *b"RSSP";
const FRAME_HEADER_LEN: usize = 12;
const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

pub trait ReplTransport {
    fn execute(&mut self, program: &[u8], state: &[u8]) -> io::Result<(i32, Vec<u8>)>;
}

pub struct SerialReplTransport<T> {
    io: T,
    device_output: Vec<u8>,
}

impl<T> SerialReplTransport<T> {
    pub fn new(io: T) -> Self {
        Self {
            io,
            device_output: Vec::new(),
        }
    }

    pub fn into_inner(self) -> T {
        self.io
    }

    pub fn take_device_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.device_output)
    }
}

impl<T: Read + Write> ReplTransport for SerialReplTransport<T> {
    fn execute(&mut self, program: &[u8], state: &[u8]) -> io::Result<(i32, Vec<u8>)> {
        let program_len = u32::try_from(program.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "VMBC frame too large"))?;
        let state_len = u32::try_from(state.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "state frame too large"))?;
        if program.len().saturating_add(state.len()) > MAX_FRAME_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "REPL request exceeds size limit",
            ));
        }

        let mut header = [0u8; FRAME_HEADER_LEN];
        header[..4].copy_from_slice(&REPL_REQUEST_MAGIC);
        header[4..8].copy_from_slice(&program_len.to_le_bytes());
        header[8..12].copy_from_slice(&state_len.to_le_bytes());
        self.io.write_all(&header)?;
        self.io.write_all(program)?;
        self.io.write_all(state)?;
        self.io.flush()?;

        self.device_output.clear();
        let mut window = [0u8; 4];
        self.io.read_exact(&mut window)?;
        while window != REPL_RESPONSE_MAGIC {
            if self.device_output.len() >= 64 * 1024 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "REPL response prelude exceeds size limit",
                ));
            }
            self.device_output.push(window[0]);
            window.rotate_left(1);
            self.io.read_exact(&mut window[3..4])?;
        }
        header[..4].copy_from_slice(&window);
        self.io.read_exact(&mut header[4..])?;
        let status = i32::from_le_bytes(header[4..8].try_into().expect("fixed slice"));
        let response_len =
            u32::from_le_bytes(header[8..12].try_into().expect("fixed slice")) as usize;
        if response_len > MAX_FRAME_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "REPL response exceeds size limit",
            ));
        }
        let mut response = vec![0; response_len];
        self.io.read_exact(&mut response)?;
        Ok((status, response))
    }
}

#[derive(Clone, Debug, PartialEq)]
struct SessionLocal {
    value: ReplValue,
    mutable: bool,
    schema: Option<TypeSchema>,
    optional: bool,
    moved: bool,
}

#[derive(Default)]
pub struct SerialReplSession {
    locals: BTreeMap<String, SessionLocal>,
}

#[derive(Debug)]
pub enum ReplClientError {
    Compile(SourceError),
    Encode(String),
    Wire(ReplWireError),
    Io(io::Error),
    Device(i32),
    MissingDebugInfo,
    MissingLocal(String),
    InvalidLocalCount { expected: usize, actual: usize },
}

impl fmt::Display for ReplClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compile(error) => write!(formatter, "{error}"),
            Self::Encode(error) => write!(formatter, "failed to encode VMBC: {error}"),
            Self::Wire(error) => write!(formatter, "invalid REPL payload: {error:?}"),
            Self::Io(error) => write!(formatter, "serial transport failed: {error}"),
            Self::Device(status) => {
                write!(formatter, "device execution failed with status {status}")
            }
            Self::MissingDebugInfo => {
                formatter.write_str("compiled REPL snippet has no debug info")
            }
            Self::MissingLocal(name) => {
                write!(formatter, "compiled REPL local '{name}' is missing")
            }
            Self::InvalidLocalCount { expected, actual } => write!(
                formatter,
                "device returned {actual} locals, expected {expected}"
            ),
        }
    }
}

impl std::error::Error for ReplClientError {}

impl From<io::Error> for ReplClientError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<ReplWireError> for ReplClientError {
    fn from(error: ReplWireError) -> Self {
        Self::Wire(error)
    }
}

impl SerialReplSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.locals.clear();
    }

    pub fn eval<T: ReplTransport>(
        &mut self,
        source: &str,
        transport: &mut T,
    ) -> Result<Option<ReplValue>, ReplClientError> {
        let compiled = compile_snippet(source, &self.locals).map_err(ReplClientError::Compile)?;
        let moved_by_rebinding =
            locals_moved_by_rebinding(&compiled.compiled.program, &self.locals);
        let program = compiled
            .compiled
            .program
            .with_local_count(compiled.compiled.locals);
        let debug = program
            .debug
            .as_ref()
            .ok_or(ReplClientError::MissingDebugInfo)?;
        let mut seed = vec![ReplValue::Null; program.local_count];
        for (name, local) in &self.locals {
            let slot = debug
                .local_index(name)
                .ok_or_else(|| ReplClientError::MissingLocal(name.clone()))?;
            seed[slot as usize] = local.value.clone();
        }

        let encoded_program = vm::encode_program(&program)
            .map_err(|error| ReplClientError::Encode(error.to_string()))?;
        let encoded_state = encode_repl_state(&seed)?;
        let (status, payload) = transport.execute(&encoded_program, &encoded_state)?;
        if status != 0 {
            return Err(ReplClientError::Device(status));
        }
        let response = decode_repl_response(&payload)?;
        if response.locals.len() != program.local_count {
            return Err(ReplClientError::InvalidLocalCount {
                expected: program.local_count,
                actual: response.locals.len(),
            });
        }
        self.sync(&program, &compiled.bindings, &moved_by_rebinding, &response)?;
        Ok(response.result)
    }

    fn sync(
        &mut self,
        program: &vm::Program,
        bindings: &[ReplLocalBinding],
        moved_by_rebinding: &BTreeSet<String>,
        response: &ReplResponse,
    ) -> Result<(), ReplClientError> {
        let debug = program
            .debug
            .as_ref()
            .ok_or(ReplClientError::MissingDebugInfo)?;
        let mut next = BTreeMap::new();
        for binding in bindings {
            let slot = debug
                .local_index(&binding.name)
                .ok_or_else(|| ReplClientError::MissingLocal(binding.name.clone()))?;
            let value = response.locals[slot as usize].clone();
            let (schema, optional) = local_schema(program, slot as usize, &value);
            let moved = moved_by_rebinding.contains(&binding.name)
                || (!optional
                    && value == ReplValue::Null
                    && matches!(schema, Some(TypeSchema::String | TypeSchema::Bytes)));
            next.insert(
                binding.name.clone(),
                SessionLocal {
                    value,
                    mutable: binding.mutable,
                    schema,
                    optional,
                    moved,
                },
            );
        }
        self.locals = next;
        Ok(())
    }
}

fn compile_snippet(
    source: &str,
    locals: &BTreeMap<String, SessionLocal>,
) -> Result<vm::CompiledReplProgram, SourceError> {
    let trimmed = source.trim_end();
    let states = locals
        .iter()
        .map(|(name, local)| ReplLocalState {
            binding: ReplLocalBinding {
                name: name.clone(),
                mutable: local.mutable,
                schema: local.schema.clone(),
                optional: local.optional,
            },
            moved: local.moved,
        })
        .collect::<Vec<_>>();
    match vm::compile_source_for_repl_with_state(trimmed, &states) {
        Ok(compiled) => Ok(compiled),
        Err(first_error) if !trimmed.ends_with(';') => {
            let fallback = format!("{trimmed};");
            match vm::compile_source_for_repl_with_state(&fallback, &states) {
                Ok(compiled) => Ok(compiled),
                Err(error @ SourceError::Parse(vm::ParseError { code: Some(_), .. })) => Err(error),
                Err(error @ SourceError::Compile(_)) => Err(error),
                Err(_) => Err(first_error),
            }
        }
        Err(error) => Err(error),
    }
}

fn locals_moved_by_rebinding(
    program: &vm::Program,
    locals: &BTreeMap<String, SessionLocal>,
) -> BTreeSet<String> {
    let Some(debug) = program.debug.as_ref() else {
        return BTreeSet::new();
    };
    let by_slot = locals
        .keys()
        .filter_map(|name| debug.local_index(name).map(|slot| (slot, name.clone())))
        .collect::<BTreeMap<_, _>>();
    let mut moved = BTreeSet::new();
    let mut ip = 0;
    while ip < program.code.len() {
        let Ok(opcode) = OpCode::try_from(program.code[ip]) else {
            break;
        };
        if opcode == OpCode::Ldloc
            && let (Some(source), Some(OpCode::Stloc), Some(target)) = (
                program.code.get(ip + 1).copied(),
                program
                    .code
                    .get(ip + 2)
                    .copied()
                    .and_then(|byte| OpCode::try_from(byte).ok()),
                program.code.get(ip + 3).copied(),
            )
            && source != target
            && let Some(name) = by_slot.get(&source)
        {
            moved.insert(name.clone());
        }
        if opcode == OpCode::Stloc
            && let Some(target) = program.code.get(ip + 1).copied()
            && let Some(name) = by_slot.get(&target)
        {
            moved.remove(name);
        }
        ip += 1 + opcode.operand_len();
    }
    moved
}

fn local_schema(
    program: &vm::Program,
    slot: usize,
    value: &ReplValue,
) -> (Option<TypeSchema>, bool) {
    let fallback = schema_from_value(value);
    let Some(type_map) = program.type_map.as_ref() else {
        return (fallback, false);
    };
    let schema = type_map
        .local_schemas
        .get(slot)
        .cloned()
        .flatten()
        .or_else(|| {
            type_map
                .local_types
                .get(slot)
                .copied()
                .and_then(schema_from_value_type)
        })
        .or(fallback);
    let optional = type_map.optional_slots.get(slot).copied().unwrap_or(false);
    (schema, optional)
}

fn schema_from_value(value: &ReplValue) -> Option<TypeSchema> {
    Some(match value {
        ReplValue::Null => TypeSchema::Null,
        ReplValue::Int(_) => TypeSchema::Int,
        ReplValue::Float(_) => TypeSchema::Float,
        ReplValue::Bool(_) => TypeSchema::Bool,
        ReplValue::String(_) => TypeSchema::String,
        ReplValue::Bytes(_) => TypeSchema::Bytes,
        ReplValue::Array(_) => TypeSchema::Array(Box::new(TypeSchema::Unknown)),
        ReplValue::Map(_) => TypeSchema::Map(Box::new(TypeSchema::Unknown)),
    })
}

fn schema_from_value_type(value_type: ValueType) -> Option<TypeSchema> {
    match value_type {
        ValueType::Unknown => None,
        ValueType::Null => Some(TypeSchema::Null),
        ValueType::Int => Some(TypeSchema::Int),
        ValueType::Float => Some(TypeSchema::Float),
        ValueType::Bool => Some(TypeSchema::Bool),
        ValueType::String => Some(TypeSchema::String),
        ValueType::Bytes => Some(TypeSchema::Bytes),
        ValueType::Array => Some(TypeSchema::Array(Box::new(TypeSchema::Unknown))),
        ValueType::Map => Some(TypeSchema::Map(Box::new(TypeSchema::Unknown))),
    }
}

pub fn is_repl_input_complete(input: &str) -> bool {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Delimiter {
        Paren,
        Bracket,
        Brace,
    }

    let mut stack = Vec::new();
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut code = String::with_capacity(input.len());
    while let Some(ch) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
                code.push('\n');
            }
            continue;
        }
        if in_block_comment {
            if ch == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block_comment = false;
            }
            continue;
        }
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
                code.push('"');
            }
            continue;
        }
        if ch == '/' {
            match chars.peek().copied() {
                Some('/') => {
                    chars.next();
                    in_line_comment = true;
                    continue;
                }
                Some('*') => {
                    chars.next();
                    in_block_comment = true;
                    continue;
                }
                _ => {}
            }
        }
        match ch {
            '"' => {
                in_string = true;
                code.push(ch);
            }
            '(' => {
                stack.push(Delimiter::Paren);
                code.push(ch);
            }
            '[' => {
                stack.push(Delimiter::Bracket);
                code.push(ch);
            }
            '{' => {
                stack.push(Delimiter::Brace);
                code.push(ch);
            }
            ')' if stack.pop() != Some(Delimiter::Paren) => return true,
            ']' if stack.pop() != Some(Delimiter::Bracket) => return true,
            '}' if stack.pop() != Some(Delimiter::Brace) => return true,
            _ => code.push(ch),
        }
    }
    if in_string || in_block_comment || !stack.is_empty() {
        return false;
    }
    let trimmed = code.trim_end();
    const INCOMPLETE: [&str; 18] = [
        "=>", "::", "&&", "||", "<=", ">=", "==", "!=", "=", ",", ".", "+", "-", "*", "/", "%",
        "!", ":",
    ];
    trimmed.is_empty() || !INCOMPLETE.iter().any(|token| trimmed.ends_with(token))
}

pub fn format_repl_value(value: &ReplValue) -> String {
    match value {
        ReplValue::Null => "null".to_string(),
        ReplValue::Int(value) => value.to_string(),
        ReplValue::Float(value) => value.to_string(),
        ReplValue::Bool(value) => value.to_string(),
        ReplValue::String(value) => value.clone(),
        ReplValue::Bytes(value) => format!("bytes[len={} hex={}]", value.len(), hex_preview(value)),
        ReplValue::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(format_repl_value)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ReplValue::Map(entries) => format!(
            "{{{}}}",
            entries
                .iter()
                .map(|(key, value)| format!(
                    "{}: {}",
                    format_repl_value(key),
                    format_repl_value(value)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn hex_preview(bytes: &[u8]) -> String {
    let mut output = String::new();
    for byte in bytes.iter().take(16) {
        use fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    if bytes.len() > 16 {
        output.push_str("..");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{decode_repl_state, encode_repl_response};

    #[derive(Default)]
    struct VmTransport {
        calls: usize,
    }

    impl ReplTransport for VmTransport {
        fn execute(&mut self, program: &[u8], state: &[u8]) -> io::Result<(i32, Vec<u8>)> {
            self.calls += 1;
            let program = vm::decode_program(program)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
            let locals = decode_repl_state(state).map_err(|error| {
                io::Error::new(io::ErrorKind::InvalidData, format!("{error:?}"))
            })?;
            let mut machine = vm::Vm::new(program);
            for (slot, value) in locals.into_iter().enumerate() {
                machine
                    .set_local(slot as u8, to_vm_value(value))
                    .map_err(|error| io::Error::other(error.to_string()))?;
            }
            machine
                .run()
                .map_err(|error| io::Error::other(error.to_string()))?;
            let response = ReplResponse {
                locals: machine.locals().iter().map(from_vm_value).collect(),
                result: machine.stack().last().map(from_vm_value),
            };
            Ok((
                0,
                encode_repl_response(&response).expect("response encodes"),
            ))
        }
    }

    fn to_vm_value(value: ReplValue) -> vm::Value {
        match value {
            ReplValue::Null => vm::Value::Null,
            ReplValue::Int(value) => vm::Value::Int(value),
            ReplValue::Float(value) => vm::Value::Float(value),
            ReplValue::Bool(value) => vm::Value::Bool(value),
            ReplValue::String(value) => vm::Value::string(value),
            ReplValue::Bytes(value) => vm::Value::bytes(value),
            ReplValue::Array(values) => {
                vm::Value::array(values.into_iter().map(to_vm_value).collect())
            }
            ReplValue::Map(entries) => vm::Value::map(
                entries
                    .into_iter()
                    .map(|(key, value)| (to_vm_value(key), to_vm_value(value)))
                    .collect(),
            ),
        }
    }

    fn from_vm_value(value: &vm::Value) -> ReplValue {
        match value {
            vm::Value::Null => ReplValue::Null,
            vm::Value::Int(value) => ReplValue::Int(*value),
            vm::Value::Float(value) => ReplValue::Float(*value),
            vm::Value::Bool(value) => ReplValue::Bool(*value),
            vm::Value::String(value) => ReplValue::String(value.as_str().to_string()),
            vm::Value::Bytes(value) => ReplValue::Bytes(value.as_ref().clone()),
            vm::Value::Array(values) => {
                ReplValue::Array(values.iter().map(from_vm_value).collect())
            }
            vm::Value::Map(entries) => ReplValue::Map(
                entries
                    .iter()
                    .map(|(key, value)| (from_vm_value(key), from_vm_value(value)))
                    .collect(),
            ),
        }
    }

    #[test]
    fn line_by_line_locals_and_results_match_desktop_repl() {
        let mut session = SerialReplSession::new();
        let mut transport = VmTransport::default();
        assert_eq!(
            session.eval("let mut x = 40;", &mut transport).unwrap(),
            None
        );
        assert_eq!(session.eval("x = x + 2;", &mut transport).unwrap(), None);
        assert_eq!(
            session.eval("x", &mut transport).unwrap(),
            Some(ReplValue::Int(42))
        );
        assert_eq!(transport.calls, 3);
    }

    #[test]
    fn compile_error_does_not_write_and_keeps_session() {
        let mut session = SerialReplSession::new();
        let mut transport = VmTransport::default();
        session.eval("let x = 7;", &mut transport).unwrap();
        assert!(session.eval("let = ;", &mut transport).is_err());
        assert_eq!(transport.calls, 1);
        assert_eq!(
            session.eval("x", &mut transport).unwrap(),
            Some(ReplValue::Int(7))
        );
    }

    #[test]
    fn moved_string_is_rejected_before_device_write() {
        let mut session = SerialReplSession::new();
        let mut transport = VmTransport::default();
        session
            .eval("let text = \"hello\";", &mut transport)
            .unwrap();
        session.eval("let other = text;", &mut transport).unwrap();
        let calls = transport.calls;
        assert!(session.eval("text", &mut transport).is_err());
        assert_eq!(transport.calls, calls);
    }

    #[test]
    fn multiline_detection_handles_delimiters_comments_and_operators() {
        assert!(!is_repl_input_complete("let x = (1 +\n"));
        assert!(is_repl_input_complete("let x = (1 +\n2);"));
        assert!(is_repl_input_complete("// { ignored\n42"));
        assert!(!is_repl_input_complete("1 +"));
        assert!(!is_repl_input_complete("/* open"));
    }

    #[test]
    fn binary_transport_uses_fixed_length_frames() {
        let response = encode_repl_response(&ReplResponse {
            locals: vec![],
            result: Some(ReplValue::Int(42)),
        })
        .unwrap();
        let mut reply = b"device log\n".to_vec();
        reply.extend_from_slice(&REPL_RESPONSE_MAGIC);
        reply.extend_from_slice(&0i32.to_le_bytes());
        reply.extend_from_slice(&(response.len() as u32).to_le_bytes());
        reply.extend_from_slice(&response);
        let io = CursorIo::new(reply);
        let mut transport = SerialReplTransport::new(io);
        let (status, payload) = transport.execute(&[0, b'\n', 255], &[1, 2]).unwrap();
        assert_eq!(status, 0);
        assert_eq!(payload, response);
        assert_eq!(transport.take_device_output(), b"device log\n");
        let io = transport.into_inner();
        assert_eq!(&io.written[..4], &REPL_REQUEST_MAGIC);
        assert_eq!(u32::from_le_bytes(io.written[4..8].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(io.written[8..12].try_into().unwrap()), 2);
        assert_eq!(&io.written[12..], &[0, b'\n', 255, 1, 2]);
    }

    struct CursorIo {
        read: std::io::Cursor<Vec<u8>>,
        written: Vec<u8>,
    }

    impl CursorIo {
        fn new(read: Vec<u8>) -> Self {
            Self {
                read: std::io::Cursor::new(read),
                written: Vec::new(),
            }
        }
    }

    impl Read for CursorIo {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.read.read(buffer)
        }
    }

    impl Write for CursorIo {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.written.extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}
