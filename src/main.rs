use std::fs;
use std::io::{self, BufRead, Write, Read};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::fmt;
use std::time::{SystemTime, Duration, Instant};
use chrono::{DateTime, Local};
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use humansize::{format_size, BINARY};
use rayon::prelude::*;

// Custom error type for the application
#[derive(Debug)]
enum AppError {
    IoError(io::Error),
    PermissionDenied(String),
    InvalidInput(String),
    FileProcessingError { path: PathBuf, error: String },
    FileSizeError(String),
    TimeoutError(String),
    EncodingError(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::IoError(err) => write!(f, "IO error: {}", err),
            AppError::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            AppError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
            AppError::FileProcessingError { path, error } => {
                write!(f, "Error processing file {:?}: {}", path, error)
            },
            AppError::FileSizeError(msg) => write!(f, "File size error: {}", msg),
            AppError::TimeoutError(msg) => write!(f, "Operation timed out: {}", msg),
            AppError::EncodingError(msg) => write!(f, "Encoding error: {}", msg),
        }
    }
}

impl Error for AppError {}

impl From<io::Error> for AppError {
    fn from(err: io::Error) -> Self {
        AppError::IoError(err)
    }
}

const MAX_FILE_SIZE: u64 = 1024 * 1024 * 1024; // 1GB
const OPERATION_TIMEOUT: Duration = Duration::from_secs(30);
const TEXT_FILE_EXTENSIONS: &[&str] = &[
    "log", "txt", "text", "err", "out", "output", "debug", 
    "conf", "config", "cfg", "ini", "properties",
    "yml", "yaml", "json", "xml", "env",
    "md", "rst", "info"
];

fn validate_file_size(size: u64, path: &Path) -> Result<()> {
    if size > MAX_FILE_SIZE {
        return Err(AppError::FileSizeError(
            format!("File {:?} exceeds maximum size limit of {}", 
                path, format_size(MAX_FILE_SIZE, BINARY))
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
mod user_privileges {
    use std::io;
    pub fn is_root_user() -> io::Result<bool> {
        Ok(unsafe { libc::geteuid() == 0 })
    }
}

type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug)]
struct LogEntry {
    line_number: usize,
    content: String,
    timestamp: Option<SystemTime>,
}

impl LogEntry {
    fn format_timestamp(&self) -> String {
        self.timestamp
            .map(|ts| {
                let datetime: DateTime<Local> = ts.into();
                datetime.format("%Y-%m-%d %H:%M:%S").to_string()
            })
            .unwrap_or_else(|| "Unknown time".to_string())
    }
}

struct ScanStats {
    total_files: usize,
    processed_files: usize,
    total_errors: usize,
    skipped_files: usize,
    large_files: usize,
}

impl ScanStats {
    fn new() -> Self {
        Self {
            total_files: 0,
            processed_files: 0,
            total_errors: 0,
            skipped_files: 0,
            large_files: 0,
        }
    }

    fn print_summary(&self, duration: Duration) {
        println!("\n{}", "📊 Scan Statistics:".cyan().bold());
        println!("├─ Scan time: {} ms", duration.as_millis().to_string().cyan());
        println!("├─ Total files scanned: {}", self.processed_files.to_string().green());
        println!("├─ Total errors found: {}", self.total_errors.to_string().yellow());
        println!("├─ Files skipped: {}", self.skipped_files.to_string().yellow());
        println!("└─ Large files encountered: {}", self.large_files.to_string().yellow());
    }
}

fn process_log_file(file_path: &Path) -> Result<Vec<LogEntry>> {
    if !file_path.exists() {
        return Err(AppError::IoError(io::Error::new(
            io::ErrorKind::NotFound,
            format!("File {:?} does not exist", file_path)
        )));
    }

    let file = fs::File::open(file_path).map_err(|e| match e.kind() {
        io::ErrorKind::PermissionDenied => 
            AppError::PermissionDenied(format!("Access denied to file {:?}", file_path)),
        io::ErrorKind::InvalidData =>
            AppError::EncodingError(format!("Invalid file encoding in {:?}", file_path)),
        _ => AppError::FileProcessingError {
            path: file_path.to_path_buf(),
            error: e.to_string(),
        },
    })?;

    let metadata = file.metadata().map_err(|e| AppError::FileProcessingError {
        path: file_path.to_path_buf(),
        error: format!("Failed to read file metadata: {}", e),
    })?;

    validate_file_size(metadata.len(), file_path)?;

    let file_size = metadata.len();
    let is_large_file = file_size > 100_000_000;

    if is_large_file {
        eprintln!("{} {} ({}) - Processing may take time...",
            "📦".yellow(),
            "Large file detected".yellow().bold(),
            format_size(file_size, BINARY).yellow());
    }

    let reader = io::BufReader::with_capacity(128 * 1024, file); // 128KB buffer
    let mut error_lines = Vec::new();
    let start_time = SystemTime::now();

    for (line_num, line_result) in reader.lines().enumerate() {
        if start_time.elapsed().map(|elapsed| elapsed > OPERATION_TIMEOUT).unwrap_or(false) {
            return Err(AppError::TimeoutError(
                format!("Processing of file {:?} timed out after {} seconds", 
                    file_path, OPERATION_TIMEOUT.as_secs())
            ));
        }

        match line_result {
            Ok(line) => {
                if line.to_lowercase().contains("error") {
                    error_lines.push(LogEntry {
                        line_number: line_num + 1,
                        content: line,
                        timestamp: metadata.modified().ok(),
                    });
                }
            }
            Err(e) => {
                match e.kind() {
                    io::ErrorKind::InvalidData => {
                        continue; // Skip invalid UTF-8 lines
                    },
                    _ => {
                        eprintln!("{} Line {} in {:?}: {}",
                            "⚠️".yellow(),
                            line_num + 1,
                            file_path,
                            e.to_string().red());
                    }
                }
            }
        }
    }

    Ok(error_lines)
}

fn get_user_confirmation() -> Result<bool> {
    let mut attempts = 0;
    const MAX_ATTEMPTS: u32 = 3;

    while attempts < MAX_ATTEMPTS {
        print!("\n{} Proceed with scanning? ({}/{}, default: y) ",
            "❓".cyan(),
            "Y".green().bold(),
            "n".red().bold());
        
        if io::stdout().flush().is_err() {
            eprintln!("{} Failed to flush stdout", "⚠️".yellow());
        }

        let mut buffer = String::new();
        match io::stdin().read_line(&mut buffer) {
            Ok(_) => {
                let choice = buffer.trim().to_lowercase();
                match choice.as_str() {
                    "" | "y" | "yes" => return Ok(true),
                    "n" | "no" => return Ok(false),
                    _ => {
                        eprintln!("{} Please enter 'y' or 'n'", "⚠️".yellow());
                        attempts += 1;
                    }
                }
            }
            Err(e) => {
                eprintln!("{} Failed to read input: {}", "⚠️".yellow(), e);
                attempts += 1;
            }
        }
    }

    Err(AppError::InvalidInput("Maximum input attempts exceeded".to_string()))
}

fn print_header() {
    println!("\n{}", "🔍 RustWatch - Log Monitor".green().bold());
    println!("{}", "=======================".green());
    println!("{} {}", "Version:".cyan(), env!("CARGO_PKG_VERSION"));
    println!("{} {}", "Time:".cyan(), Local::now().format("%Y-%m-%d %H:%M:%S"));
    println!("\n{}", "RustWatch vigilantly monitors your logs for errors and issues.".italic());
    println!("{}", "Scan system logs or any directory with lightning speed.".italic());
}

fn get_scan_directory() -> Result<PathBuf> {
    println!("\n{}", "📂 Select scan location:".cyan().bold());
    println!("  {} Default location (/var/log) {}", "1.".cyan().bold(), "(default)".cyan().italic());
    println!("  {} Custom directory", "2.".cyan());

    let mut attempts = 0;
    const MAX_ATTEMPTS: u32 = 3;

    while attempts < MAX_ATTEMPTS {
        print!("\n{} Choose an option (1/2, default: 1): ", "❓".cyan());
        if io::stdout().flush().is_err() {
            eprintln!("{} Failed to flush stdout", "⚠️".yellow());
        }

        let mut buffer = String::new();
        match io::stdin().read_line(&mut buffer) {
            Ok(_) => {
                match buffer.trim() {
                    "" | "1" => return Ok(PathBuf::from("/var/log")),
                    "2" => {
                        print!("\n{} Enter directory path: ", "📁".cyan());
                        if io::stdout().flush().is_err() {
                            eprintln!("{} Failed to flush stdout", "⚠️".yellow());
                        }

                        let mut path_buffer = String::new();
                        match io::stdin().read_line(&mut path_buffer) {
                            Ok(_) => {
                                let path = PathBuf::from(path_buffer.trim());
                                if !path.exists() {
                                    eprintln!("{} Directory does not exist", "❌".red());
                                    attempts += 1;
                                    continue;
                                }
                                if !path.is_dir() {
                                    eprintln!("{} Path is not a directory", "❌".red());
                                    attempts += 1;
                                    continue;
                                }
                                return Ok(path);
                            }
                            Err(e) => {
                                eprintln!("{} Failed to read input: {}", "⚠️".yellow(), e);
                                attempts += 1;
                            }
                        }
                    }
                    _ => {
                        eprintln!("{} Please enter 1 or 2", "⚠️".yellow());
                        attempts += 1;
                    }
                }
            }
            Err(e) => {
                eprintln!("{} Failed to read input: {}", "⚠️".yellow(), e);
                attempts += 1;
            }
        }
    }

    Err(AppError::InvalidInput("Maximum attempts exceeded while selecting directory".to_string()))
}

fn is_text_file(path: &Path) -> bool {
    // Check extension first
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if TEXT_FILE_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
            return true;
        }
    }

    // If no extension or not in list, try to read first few bytes
    if let Ok(mut file) = fs::File::open(path) {
        let mut buffer = [0; 512];
        if let Ok(size) = file.read(&mut buffer) {
            if size == 0 { return false; }  // Empty file
            
            // Check for null bytes and high concentration of non-ASCII chars
            let null_bytes = buffer[..size].iter().filter(|&&b| b == 0).count();
            let non_ascii = buffer[..size].iter().filter(|&&b| b > 127).count();
            
            // If more than 1% null bytes or 30% non-ASCII, probably binary
            return (null_bytes as f32 / size as f32) < 0.01 
                && (non_ascii as f32 / size as f32) < 0.3;
        }
    }
    false
}

fn collect_files_recursive(dir_path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    match fs::read_dir(dir_path) {
        Ok(entries) => {
            for entry in entries {
                match entry {
                    Ok(entry) => {
                        let path = entry.path();
                        if path.is_file() {
                            // Only add if it's a text file
                            if is_text_file(&path) {
                                files.push(path);
                            } else {
                                // Optional: uncomment to see which files are skipped
                                // eprintln!("{} Skipping non-text file: {}",
                                //     "ℹ️".blue(),
                                //     path.display());
                            }
                        } else if path.is_dir() {
                            // If we can't access a subdirectory, log it and continue
                            if let Err(e) = collect_files_recursive(&path, files) {
                                match e {
                                    AppError::PermissionDenied(_) => {
                                        eprintln!("{} Skipping directory {}: {}",
                                            "⚠️".yellow(),
                                            path.display(),
                                            "Permission denied".yellow());
                                    },
                                    _ => {
                                        eprintln!("{} Error accessing directory {}: {}",
                                            "⚠️".yellow(),
                                            path.display(),
                                            e.to_string().red());
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        match e.kind() {
                            io::ErrorKind::PermissionDenied => {
                                eprintln!("{} Skipping entry in {}: {}",
                                    "⚠️".yellow(),
                                    dir_path.display(),
                                    "Permission denied".yellow());
                            },
                            _ => {
                                eprintln!("{} Error accessing entry in {}: {}",
                                    "⚠️".yellow(),
                                    dir_path.display(),
                                    e.to_string().red());
                            }
                        }
                    }
                }
            }
            Ok(())
        }
        Err(e) => {
            match e.kind() {
                io::ErrorKind::PermissionDenied => {
                    Err(AppError::PermissionDenied(
                        format!("Cannot access directory {}: Permission denied", dir_path.display())
                    ))
                },
                _ => Err(AppError::IoError(e))
            }
        }
    }
}

fn main() -> Result<()> {
    print_header();

    #[cfg(target_os = "linux")]
    if let Ok(is_root) = user_privileges::is_root_user() {
        if !is_root {
            eprintln!("\n{} {} This tool is not running with sudo privileges.",
                "⚠️".yellow(),
                "Warning:".yellow().bold());
            eprintln!("{} Some directories may not be accessible. Run with sudo for full access.\n",
                " ".repeat(9));
        }
    }

    let log_dir_path = get_scan_directory()?;
    println!("\n{} Scanning directory: {}", "📂".cyan(), log_dir_path.display());

    if !log_dir_path.exists() {
        return Err(AppError::IoError(io::Error::new(
            io::ErrorKind::NotFound,
            format!("❌ Directory {} does not exist", log_dir_path.display())
        )));
    }

    let mut log_files = Vec::new();
    println!("{}", "🔍 Scanning directory tree...".cyan());
    collect_files_recursive(&log_dir_path, &mut log_files)?;

    if log_files.is_empty() {
        return Err(AppError::IoError(io::Error::new(
            io::ErrorKind::Other,
            "❌ No readable files found"
        )));
    }

    log_files.sort_by(|a, b| a.display().to_string().cmp(&b.display().to_string()));

    println!("\n{}", "📁 Files to be scanned:".cyan().bold());
    for (i, file) in log_files.iter().enumerate() {
        let display_path = file.strip_prefix(&log_dir_path)
            .unwrap_or(file)
            .display();
        println!("  {} {} {}", 
            "└─".cyan(),
            format!("[{:02}]", i + 1).blue(),
            display_path);
    }

    if !get_user_confirmation()? {
        println!("{} {}", "✋".yellow(), "Scan cancelled by user.".yellow());
        return Ok(());
    }

    println!("\n{}", "🚀 Starting scan...".cyan().bold());
    let start_time = Instant::now();

    let pb = ProgressBar::new(log_files.len() as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
        .unwrap()
        .progress_chars("█▇▆▅▄▃▂▁"));

    let mut stats = ScanStats::new();
    stats.total_files = log_files.len();

    // Process files in parallel
    let results: Vec<_> = log_files.par_iter()
        .map(|file_path| {
            let result = process_log_file(file_path);
            pb.inc(1);
            (file_path, result)
        })
        .collect();

    pb.finish_with_message("✅ Scan complete");

    let mut errors_by_file = Vec::new();

    for (file_path, result) in results {
        match result {
            Ok(error_lines) => {
                if !error_lines.is_empty() {
                    let display_path = file_path.strip_prefix(&log_dir_path)
                        .unwrap_or(file_path)
                        .display()
                        .to_string();
                    stats.total_errors += error_lines.len();
                    errors_by_file.push((display_path, error_lines));
                }
                stats.processed_files += 1;
            }
            Err(e) => {
                eprintln!("{} {}: {}",
                    "❌".red(),
                    file_path.display(),
                    e.to_string().red());
                stats.skipped_files += 1;
            }
        }
    }

    if stats.processed_files == 0 {
        return Err(AppError::IoError(io::Error::new(
            io::ErrorKind::Other,
            "❌ Could not process any files"
        )));
    }

    if stats.total_errors > 0 {
        println!("\n{}", "🔍 Errors Found:".cyan().bold());
        println!("{}", "==============".cyan());
        
        for (file_name, error_lines) in &errors_by_file {
            if !error_lines.is_empty() {
                println!("\n{} {} ({} {})", 
                    "📄".cyan(),
                    file_name.bold(),
                    error_lines.len(),
                    if error_lines.len() == 1 { "error" } else { "errors" });

                for entry in error_lines {
                    println!("  {} {} - [{}] {}",
                        "└─".cyan(),
                        format!("Line {}", entry.line_number).yellow(),
                        entry.format_timestamp().blue(),
                        entry.content.red());
                }
            }
        }
    } else {
        println!("\n{} {}", "✅".green(), "No errors found in processed files.".green());
    }

    let duration = start_time.elapsed();
    stats.print_summary(duration);
    
    Ok(())
}