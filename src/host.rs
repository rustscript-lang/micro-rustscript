use std::fmt;
use std::path::Path;

pub use vm::Value;
use vm::{
    JitConfig, SourceMap, Vm, VmStatus, compile_source, compile_source_file,
    compile_source_for_repl, render_source_error,
};

#[derive(Debug, Clone, PartialEq)]
pub enum RunOutcome {
    Halted { stack: Vec<Value> },
}

#[derive(Debug)]
pub enum EmbeddedError {
    Compile(String),
    Runtime(String),
    Io(std::io::Error),
}

impl fmt::Display for EmbeddedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compile(message) | Self::Runtime(message) => f.write_str(message),
            Self::Io(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for EmbeddedError {}

impl From<std::io::Error> for EmbeddedError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

pub type EmbeddedResult<T> = Result<T, EmbeddedError>;

pub fn run_source(source: &str) -> EmbeddedResult<RunOutcome> {
    let compiled = compile_source(source)
        .map_err(|err| render_source_compile_error("<memory>", source, err))?;
    run_program(compiled.program.with_local_count(compiled.locals))
}

pub fn run_source_file(path: impl AsRef<Path>) -> EmbeddedResult<RunOutcome> {
    let path = path.as_ref();
    let compiled =
        compile_source_file(path).map_err(|err| EmbeddedError::Compile(err.to_string()))?;
    run_program(compiled.program.with_local_count(compiled.locals))
}

pub fn eval_repl_entry(source: &str) -> EmbeddedResult<RunOutcome> {
    let compiled = compile_source_for_repl(source)
        .map_err(|err| render_source_compile_error("<repl>", source, err))?;
    run_program(compiled.program.with_local_count(compiled.locals))
}

pub fn embedded_jit_config() -> JitConfig {
    JitConfig {
        enabled: false,
        ..JitConfig::default()
    }
}

pub fn new_vm(program: vm::Program) -> Vm {
    let mut vm = Vm::new_with_jit_config(program, embedded_jit_config());
    vm.set_runtime_print_sink(|rendered| {
        print!("{rendered}");
    });
    vm
}

pub fn render_value(value: &Value) -> String {
    vm::format_value(value)
}

fn run_program(program: vm::Program) -> EmbeddedResult<RunOutcome> {
    let mut vm = new_vm(program);
    loop {
        match vm.run() {
            Ok(VmStatus::Halted) => {
                return Ok(RunOutcome::Halted {
                    stack: vm.stack().to_vec(),
                });
            }
            Ok(VmStatus::Yielded) => continue,
            Ok(VmStatus::Waiting(_)) => vm
                .wait_for_host_op_blocking()
                .map_err(|err| EmbeddedError::Runtime(vm::render_vm_error(&vm, &err)))?,
            Err(err) => return Err(EmbeddedError::Runtime(vm::render_vm_error(&vm, &err))),
        }
    }
}

fn render_source_compile_error(path: &str, source: &str, err: vm::SourceError) -> EmbeddedError {
    match err {
        vm::SourceError::Parse(parse) => {
            let mut source_map = SourceMap::new();
            let source_id = source_map.add_source(path.to_string(), source.to_string());
            let parse = parse.with_line_span_from_source(&source_map, source_id);
            EmbeddedError::Compile(render_source_error(&source_map, &parse, true))
        }
        vm::SourceError::Compile(compile) => {
            let mut source_map = SourceMap::new();
            source_map.add_source(path.to_string(), source.to_string());
            EmbeddedError::Compile(vm::render_compile_error(&source_map, &compile, true))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jit_is_disabled() {
        assert!(!embedded_jit_config().enabled);
    }

    #[test]
    fn runs_basic_program() {
        let _ = eval_repl_entry("print(1 + 2);").expect("program should run");
    }
}
