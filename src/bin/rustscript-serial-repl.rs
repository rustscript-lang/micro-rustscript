use std::io::{self, Write};
use std::time::Duration;

use rustscript_embedded::{
    SerialReplSession, SerialReplTransport, format_repl_value, is_repl_input_complete,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let port = args
        .next()
        .ok_or("usage: rustscript-serial-repl <port> [baud]")?;
    let baud = args
        .next()
        .map(|value| value.parse::<u32>())
        .transpose()?
        .unwrap_or(115_200);
    if args.next().is_some() {
        return Err("usage: rustscript-serial-repl <port> [baud]".into());
    }

    let serial = serialport::new(&port, baud)
        .timeout(Duration::from_secs(10))
        .open()?;
    serial.clear(serialport::ClearBuffer::Input)?;
    let mut transport = SerialReplTransport::new(serial);
    let mut session = SerialReplSession::new();
    let mut pending = String::new();
    let stdin = io::stdin();

    println!("RustScript serial REPL on {port} ({baud} baud)");
    println!("commands: .help, .cancel, .clear, .quit");
    loop {
        print!("{}", if pending.is_empty() { "rss> " } else { "...> " });
        io::stdout().flush()?;
        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            println!();
            break;
        }
        let command = line.trim();
        match command {
            ".quit" | ".exit" => break,
            ".help" if pending.is_empty() => {
                println!(".cancel clears multiline input; .clear clears saved locals; .quit exits");
                continue;
            }
            ".cancel" => {
                pending.clear();
                continue;
            }
            ".clear" if pending.is_empty() => {
                session.clear();
                println!("session cleared");
                continue;
            }
            _ if command.starts_with('.') && pending.is_empty() => {
                println!("unknown command: {command}");
                continue;
            }
            _ => {}
        }
        if line.trim().is_empty() && pending.is_empty() {
            continue;
        }
        pending.push_str(&line);
        if !is_repl_input_complete(&pending) {
            continue;
        }
        let snippet = std::mem::take(&mut pending);
        let result = session.eval(&snippet, &mut transport);
        let device_output = transport.take_device_output();
        if !device_output.is_empty() {
            io::stdout().write_all(&device_output)?;
            if !device_output.ends_with(b"\n") {
                println!();
            }
        }
        match result {
            Ok(Some(value)) => println!("=> {}", format_repl_value(&value)),
            Ok(None) => {}
            Err(error) => eprintln!("error: {error}"),
        }
    }
    Ok(())
}
