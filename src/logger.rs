//! Модуль логування: дописує події у текстовий файл.
//!
//! Формат рядка: `[2025-01-31 14:05:09] [INFO] Повідомлення`

use chrono::Local;
use std::fmt;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Рівень важливості запису в журналі.
#[derive(Debug, Clone, Copy)]
pub enum Level {
    Info,
    Error,
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Level::Info => "INFO",
            Level::Error => "ERROR",
        };
        write!(f, "{text}")
    }
}

/// Простий файловий логер.
pub struct Logger {
    path: PathBuf,
}

impl Logger {
    /// Створює логер, який писатиме у вказаний файл.
    /// Сам файл створюється при першому записі.
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Logger {
            path: path.as_ref().to_path_buf(),
        }
    }

    /// Записує інформаційне повідомлення (рівень INFO).
    pub fn info(&self, message: &str) {
        self.log(Level::Info, message);
    }

    /// Записує повідомлення про помилку (рівень ERROR).
    pub fn error(&self, message: &str) {
        self.log(Level::Error, message);
    }

    /// Публічний запис, що не "валить" програму:
    /// якщо журнал недоступний, лише виводить попередження в stderr.
    fn log(&self, level: Level, message: &str) {
        if let Err(e) = self.try_log(level, message) {
            eprintln!(
                "Попередження: не вдалося записати в журнал '{}': {}",
                self.path.display(),
                e
            );
        }
    }

    /// Внутрішня реалізація, що повертає Result.
    /// Файл відкривається в режимі дописування (append), тож старі записи зберігаються.
    fn try_log(&self, level: Level, message: &str) -> io::Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;

        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        writeln!(file, "[{timestamp}] [{level}] {message}")
    }
}
