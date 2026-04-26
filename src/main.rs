use std::path::PathBuf;
use clap::{Parser, Subcommand};
use gate::gate::{lexer::Lexer, parser::Parser as GateParser, interpreter::{Interpreter, Value}};

#[derive(Parser)]
#[command(name = "gate")]
#[command(version, about = "Gate — a DSL for version control workflows")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a workflow from a .gate file
    Run {
        /// Path to the .gate file
        file: PathBuf,

        /// Workflow to run (defaults to "main")
        #[arg(default_value = "main")]
        workflow: String,

        /// Arguments to pass to the workflow (key=value)
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,

        /// Dry run — print torii commands without executing
        #[arg(long)]
        dry_run: bool,
    },

    /// Check a .gate file for syntax errors without running it
    Check {
        /// Path to the .gate file
        file: PathBuf,
    },

    /// List all workflows defined in a .gate file
    List {
        /// Path to the .gate file
        file: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli) {
        eprintln!("❌ {}", e);
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Run { file, workflow, args, dry_run } => {
            let program = load_file(&file)?;
            let mut interp = Interpreter::new();
            interp.dry_run = dry_run;
            interp.load(program);

            let (positional, named) = parse_run_args(&args);

            println!("▶  Running workflow '{}'...\n", workflow);
            interp.run_with_named(&workflow, positional, &named)
                .map_err(|e| e.to_string())?;
            println!("\n✅ Done");
            Ok(())
        }

        Command::Check { file } => {
            load_file(&file)?;
            println!("✅ {} — no syntax errors", file.display());
            Ok(())
        }

        Command::List { file } => {
            let source = read_file(&file)?;
            let mut lexer = Lexer::new(&source);
            let tokens = lexer.tokenize().map_err(|e| e.to_string())?;
            let mut parser = GateParser::new(tokens);
            let program = parser.parse().map_err(|e| e.to_string())?;

            println!("📋 Workflows in {}:\n", file.display());
            let mut found = false;
            for item in &program.items {
                if let gate::gate::ast::Item::Workflow(w) = item {
                    found = true;
                    let params: Vec<String> = w.params.iter().map(|p| {
                        if p.default.is_some() {
                            format!("{} = ...", p.name)
                        } else {
                            p.name.clone()
                        }
                    }).collect();
                    println!("  {} ({})", w.name, params.join(", "));
                }
            }
            if !found {
                println!("  (no workflows found)");
            }
            Ok(())
        }
    }
}

/// Split user-supplied workflow args into positional values and `key=value`
/// named values. Bare tokens become positional strings; `key=value` becomes a
/// named binding for that workflow parameter. Values keep string form — the
/// interpreter coerces where needed (numbers, bools, etc. require explicit
/// args inside `.gate` files for now).
fn parse_run_args(args: &[String]) -> (Vec<Value>, Vec<(String, Value)>) {
    let mut positional = Vec::new();
    let mut named = Vec::new();
    for arg in args {
        if let Some((k, v)) = arg.split_once('=') {
            let k = k.trim();
            if !k.is_empty() && k.chars().all(|c| c.is_alphanumeric() || c == '_') {
                named.push((k.to_string(), Value::String(v.to_string())));
                continue;
            }
        }
        positional.push(Value::String(arg.clone()));
    }
    (positional, named)
}

fn read_file(path: &PathBuf) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{}': {}", path.display(), e))
}

fn load_file(path: &PathBuf) -> Result<gate::gate::ast::Program, String> {
    let source = read_file(path)?;
    let mut lexer = Lexer::new(&source);
    let tokens = lexer.tokenize().map_err(|e| e.to_string())?;
    let mut parser = GateParser::new(tokens);
    parser.parse().map_err(|e| e.to_string())
}
