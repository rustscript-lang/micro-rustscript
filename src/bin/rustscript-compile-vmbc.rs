use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let source =
        PathBuf::from(args.next().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "missing input RSS path")
        })?);
    let output =
        PathBuf::from(args.next().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "missing output VMBC path")
        })?);
    if args.next().is_some() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "too many arguments").into());
    }

    let compiled = vm::compile_source_file(&source).map_err(|error| {
        io::Error::other(format!("failed to compile {}: {error:?}", source.display()))
    })?;
    let encoded = vm::encode_program(&compiled.program)
        .map_err(|error| io::Error::other(format!("failed to encode VMBC: {error:?}")))?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output, &encoded)?;
    println!("wrote {} bytes to {}", encoded.len(), output.display());
    Ok(())
}
