use std::io::{self, BufRead, Write};

use rustscript_embedded::{RunOutcome, eval_repl_entry, render_value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("RustScript embedded REPL (JIT off)");
    println!("commands: .help, .quit");

    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut line = String::new();

    loop {
        print!("rs-emb> ");
        io::stdout().flush()?;
        line.clear();
        if input.read_line(&mut line)? == 0 {
            println!();
            break;
        }

        let source = line.trim();
        match source {
            "" => continue,
            ".quit" | ".exit" => break,
            ".help" => {
                println!("enter RustScript statements, for example: print(1 + 2);");
                continue;
            }
            _ => {}
        }

        match eval_repl_entry(source)? {
            RunOutcome::Halted { stack } => {
                if let Some(value) = stack.last() {
                    println!("=> {}", render_value(value));
                } else {
                    println!("=> <empty>");
                }
            }
        }
    }

    Ok(())
}
