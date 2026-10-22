//! Simple File Archiver — консольний архіватор на базі Gzip.
//!
//! Використання:
//!   simple_archiver compress   <input_file>    <output_archive>
//!   simple_archiver decompress <input_archive> <output_file>

mod logger;
mod processor;

use logger::Logger;
use std::env;
use std::error::Error;
use std::path::Path;
use std::process::ExitCode;

/// Назва файлу журналу (створюється в поточній директорії).
const LOG_FILE: &str = "archiver.log";

const USAGE: &str = "Використання:\n  \
    simple_archiver compress   <вхідний_файл>  <вихідний_архів>\n  \
    simple_archiver decompress <вхідний_архів> <вихідний_файл>";

fn main() -> ExitCode {
    let logger = Logger::new(LOG_FILE);

    // Усі помилки збираються тут і виводяться в зрозумілому вигляді.
    match run(&logger) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Помилка: {e}");
            logger.error(&e.to_string());
            ExitCode::FAILURE
        }
    }
}

/// Розбирає аргументи командного рядка та виконує команду.
fn run(logger: &Logger) -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();

    // Очікуємо рівно 3 аргументи після імені програми.
    let (command, input, output) = match args.as_slice() {
        [_, command, input, output] => (command.as_str(), input.as_str(), output.as_str()),
        _ => return Err(format!("невірна кількість аргументів.\n{USAGE}").into()),
    };

    let input_path = Path::new(input);
    let output_path = Path::new(output);

    match command {
        "compress" => {
            logger.info(&format!("Початок стиснення: '{input}' -> '{output}'"));
            let stats = processor::compress_file(input_path, output_path)?;
            let msg = format!(
                "Стиснення завершено: {} Б -> {} Б ('{output}')",
                stats.input_bytes, stats.output_bytes
            );
            logger.info(&msg);
            println!("{msg}");
        }
        "decompress" => {
            logger.info(&format!("Початок розпакування: '{input}' -> '{output}'"));
            let stats = processor::decompress_file(input_path, output_path)?;
            let msg = format!(
                "Розпакування завершено: {} Б -> {} Б ('{output}')",
                stats.input_bytes, stats.output_bytes
            );
            logger.info(&msg);
            println!("{msg}");
        }
        other => {
            return Err(format!("невідома команда '{other}'.\n{USAGE}").into());
        }
    }

    Ok(())
}
