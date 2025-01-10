# 🦊 RustWatch

[![Rust](https://img.shields.io/badge/rust-stable-brightgreen.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Version](https://img.shields.io/badge/version-0.1.0-blue.svg)](https://github.com/JohnSoellnerDev/RustWatch)

A lightning-fast, parallel log file error scanner built in Rust. RustWatch helps you monitor and analyze log files efficiently by scanning for errors and issues across multiple files simultaneously.

## ✨ Features

- 🚀 **Lightning Fast**: Parallel processing of log files using Rayon
- 📁 **Flexible Scanning**: Scan system logs or any custom directory
- 🎨 **Beautiful Interface**: Colorful, intuitive CLI with progress indicators
- 🛡️ **Robust Error Handling**: Comprehensive error handling and recovery
- 📊 **Detailed Statistics**: Get insights about your scan results
- 💻 **Cross-Platform**: Works on Linux and Windows

## 🚀 Installation

### From Source
```bash
# Clone the repository
git clone https://github.com/JohnSoellnerDev/RustWatch
cd rustwatch

# Build and install
cargo install --path .
```

## 📊 Output Example

```
🦊 RustWatch - Log Monitor
=======================
Version: 0.1.0
Time: 2024-12-20 15:30:45

📁 Found these files to scan:
  └─ [01] system.log
  └─ [02] error.log
  ...

📊 Scan Statistics:
├─ Scan time: 1234 ms
├─ Total files scanned: 42
├─ Total errors found: 7
├─ Files skipped: 2
└─ Large files encountered: 1
```

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request. For major changes, please open an issue first to discuss what you would like to change.

## 📝 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- Built with [Rust](https://www.rust-lang.org/)
- Parallel processing powered by [Rayon](https://github.com/rayon-rs/rayon)
- CLI interface enhanced by [colored](https://github.com/mackwic/colored)
- Progress bars by [indicatif](https://github.com/console-rs/indicatif)

---
Made with ❤️ by [JohnSoellnerDev](hhttps://github.com/JohnSoellnerDev) 
