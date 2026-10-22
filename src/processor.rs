//! Модуль обробки файлів: стиснення та розпакування (Gzip через `flate2`).
//!
//! Усі операції потокові: дані читаються й записуються блоками через
//! `BufReader` / `BufWriter` та `std::io::copy`, тому розмір файлу
//! не впливає на споживання оперативної пам'яті.

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Write};
use std::path::Path;

/// Власний тип помилки для всіх операцій архіватора.
#[derive(Debug)]
pub enum ArchiverError {
    /// Файл не існує.
    NotFound(String),
    /// Недостатньо прав доступу.
    PermissionDenied(String),
    /// Архів пошкоджений або не є Gzip-файлом.
    CorruptArchive(String),
    /// Вхідний і вихідний файли — це той самий файл.
    SamePath(String),
    /// Будь-яка інша помилка вводу-виводу.
    Io { path: String, source: io::Error },
}

impl ArchiverError {
    /// Перетворює `io::Error` у зрозумілішу помилку, враховуючи її тип (kind).
    fn classify(err: io::Error, path: &Path) -> Self {
        let p = path.display().to_string();
        match err.kind() {
            io::ErrorKind::NotFound => ArchiverError::NotFound(p),
            io::ErrorKind::PermissionDenied => ArchiverError::PermissionDenied(p),
            _ => ArchiverError::Io { path: p, source: err },
        }
    }

    /// Те саме, але для читання архіву: помилки формату Gzip
    /// (InvalidData / InvalidInput / UnexpectedEof) означають "битий архів".
    fn corrupt_or_io(err: io::Error, path: &Path) -> Self {
        match err.kind() {
            io::ErrorKind::InvalidData
            | io::ErrorKind::InvalidInput
            | io::ErrorKind::UnexpectedEof => {
                ArchiverError::CorruptArchive(path.display().to_string())
            }
            _ => Self::classify(err, path),
        }
    }
}

impl fmt::Display for ArchiverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArchiverError::NotFound(p) => write!(f, "файл '{p}' не знайдено"),
            ArchiverError::PermissionDenied(p) => {
                write!(f, "відмовлено у доступі до '{p}'")
            }
            ArchiverError::CorruptArchive(p) => {
                write!(f, "архів '{p}' пошкоджений або має невірний формат")
            }
            ArchiverError::SamePath(p) => {
                write!(f, "вхідний і вихідний файли збігаються: '{p}'")
            }
            ArchiverError::Io { path, source } => {
                write!(f, "помилка вводу-виводу з файлом '{path}': {source}")
            }
        }
    }
}

impl Error for ArchiverError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ArchiverError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Статистика успішної операції (розміри в байтах).
#[derive(Debug, Clone, Copy)]
pub struct Stats {
    /// Розмір вхідного файлу.
    pub input_bytes: u64,
    /// Розмір створеного файлу.
    pub output_bytes: u64,
}

/// Стискає `input` у Gzip-архів `output`.
pub fn compress_file(input: &Path, output: &Path) -> Result<Stats, ArchiverError> {
    ensure_different_files(input, output)?;

    // Спершу відкриваємо вхід: якщо його немає, існуючий `output` не чіпаємо.
    let input_file = File::open(input).map_err(|e| ArchiverError::classify(e, input))?;
    let output_file = File::create(output).map_err(|e| ArchiverError::classify(e, output))?;

    let result = compress_stream(input_file, output_file, input, output);

    // Якщо щось пішло не так — прибираємо недописаний архів (best effort).
    if result.is_err() {
        let _ = fs::remove_file(output);
    }
    result
}

/// Розпаковує Gzip-архів `input` у файл `output`.
pub fn decompress_file(input: &Path, output: &Path) -> Result<Stats, ArchiverError> {
    ensure_different_files(input, output)?;

    let input_file = File::open(input).map_err(|e| ArchiverError::classify(e, input))?;
    let output_file = File::create(output).map_err(|e| ArchiverError::classify(e, output))?;

    let result = decompress_stream(input_file, output_file, input, output);

    if result.is_err() {
        let _ = fs::remove_file(output);
    }
    result
}

/// Потокове стиснення: BufReader -> GzEncoder -> BufWriter -> файл.
fn compress_stream(
    input_file: File,
    output_file: File,
    input: &Path,
    output: &Path,
) -> Result<Stats, ArchiverError> {
    let mut reader = BufReader::new(input_file);
    let mut encoder = GzEncoder::new(BufWriter::new(output_file), Compression::default());

    // Копіюємо блоками; весь файл у пам'ять не завантажується.
    let input_bytes = io::copy(&mut reader, &mut encoder)
        .map_err(|e| ArchiverError::classify(e, input))?;

    // finish() дописує Gzip-трейлер і повертає внутрішній BufWriter.
    let mut writer = encoder
        .finish()
        .map_err(|e| ArchiverError::classify(e, output))?;
    // Явний flush, щоб помилки запису на диск не губилися при Drop.
    writer
        .flush()
        .map_err(|e| ArchiverError::classify(e, output))?;

    let output_bytes = file_size(output)?;
    Ok(Stats {
        input_bytes,
        output_bytes,
    })
}

/// Потокове розпакування: файл -> BufReader -> GzDecoder -> BufWriter -> файл.
fn decompress_stream(
    input_file: File,
    output_file: File,
    input: &Path,
    output: &Path,
) -> Result<Stats, ArchiverError> {
    let reader = BufReader::new(input_file);
    let mut decoder = GzDecoder::new(reader);
    let mut writer = BufWriter::new(output_file);

    // Помилки декодування (битий заголовок, CRC тощо) потрапляють сюди.
    io::copy(&mut decoder, &mut writer)
        .map_err(|e| ArchiverError::corrupt_or_io(e, input))?;
    writer
        .flush()
        .map_err(|e| ArchiverError::classify(e, output))?;

    Ok(Stats {
        input_bytes: file_size(input)?,
        output_bytes: file_size(output)?,
    })
}

/// Розмір файлу через метадані (вміст не читається).
fn file_size(path: &Path) -> Result<u64, ArchiverError> {
    fs::metadata(path)
        .map(|m| m.len())
        .map_err(|e| ArchiverError::classify(e, path))
}

/// Захист від випадку `compress a.txt a.txt`: `File::create` обнулив би вхідний файл.
fn ensure_different_files(input: &Path, output: &Path) -> Result<(), ArchiverError> {
    // canonicalize вдається лише для існуючих файлів; якщо output ще не існує — все гаразд.
    if let (Ok(a), Ok(b)) = (fs::canonicalize(input), fs::canonicalize(output)) {
        if a == b {
            return Err(ArchiverError::SamePath(input.display().to_string()));
        }
    }
    Ok(())
}
