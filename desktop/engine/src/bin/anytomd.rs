//! `anytomd` command line: convert a folder (mirrored) or a single file.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use anytomd_engine::{convert_folder, convert_single_file, default_output, Job, Options, VERSION};

const HELP: &str = "\
Any File To MD - convert files to Markdown, locally.

USAGE:
    anytomd <folder-or-file> [options]

A folder is mirrored into <folder>-markdown (originals are never touched).
A single file becomes <name>.md next to it.

OPTIONS:
    -o, --output <path>     Output folder (or output file for a single file)
        --all               Re-convert everything (default: skip unchanged files)
        --combine           Also write _combined.md with every document and a table of contents
        --copy-unsupported  Copy images, media and other unconvertible files as-is
        --include <exts>    Only these extensions, e.g. --include pdf,docx,pptx
        --exclude <glob>    Skip matching paths, e.g. --exclude \"*.log\" (repeatable)
    -q, --quiet             Only print errors
    -h, --help              Show this help
    -V, --version           Show the version

EXIT CODES: 0 success, 1 error, 2 finished but some files could not be converted.";

fn main() -> ExitCode {
    std::panic::set_hook(Box::new(|_| {})); // parser panics are reported per file
    let mut args = std::env::args().skip(1);
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut opts = Options::default();
    let mut quiet = false;
    while let Some(a) = args.next() {
        match a.as_str() {
            "-h" | "--help" | "/?" => {
                println!("{HELP}");
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("anytomd {VERSION}");
                return ExitCode::SUCCESS;
            }
            "-o" | "--output" => output = args.next().map(PathBuf::from),
            "--all" => opts.skip_unchanged = false,
            "--combine" => opts.combine = true,
            "--copy-unsupported" => opts.copy_unsupported = true,
            "--include" => opts.include.extend(args.next()),
            "--exclude" => opts.exclude.extend(args.next()),
            "-q" | "--quiet" => quiet = true,
            s if s.starts_with('-') => {
                eprintln!("Unknown option {s}. Run anytomd --help.");
                return ExitCode::from(1);
            }
            s => input = Some(PathBuf::from(s.trim_matches('"'))),
        }
    }
    let Some(input) = input else {
        println!("{HELP}");
        return ExitCode::from(1);
    };

    if input.is_file() {
        return match convert_single_file(&input, output.as_deref()) {
            Ok(dest) => {
                if !quiet {
                    println!("{}", dest.display());
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{}: {e}", input.display());
                ExitCode::from(2)
            }
        };
    }

    let output = output.unwrap_or_else(|| default_output(&input));
    let job = Job::new();
    let worker = {
        let (job, input, output, opts) = (job.clone(), input.clone(), output.clone(), opts.clone());
        std::thread::spawn(move || convert_folder(&input, &output, &opts, &job))
    };
    let mut err = std::io::stderr();
    while !worker.is_finished() {
        std::thread::sleep(Duration::from_millis(150));
        if !quiet {
            let (s, _, _) = job.snapshot(usize::MAX);
            if s.total > 0 {
                let _ = write!(err, "\r  {:>3}%  {}/{} files ", 100 * s.done / s.total.max(1), s.done, s.total);
                let _ = err.flush();
            }
        }
    }
    let _ = worker.join();
    let (s, _, _) = job.snapshot(usize::MAX);
    if s.state == "error" {
        eprintln!("Error: {}", s.error);
        return ExitCode::from(1);
    }
    if !quiet {
        eprintln!("\r                                        ");
        println!(
            "Done in {}s: {} converted, {} unchanged, {} copied, {} skipped.\nOutput: {}",
            s.seconds, s.converted, s.unchanged, s.copied, s.failed, s.output
        );
        if let Some(c) = &s.combined {
            println!("Combined: {c}");
        }
    }
    for f in &s.failures {
        eprintln!("  skipped {}: {}", f.file, f.reason);
    }
    if s.failed > 0 {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}
