//! 当前平台描述：操作系统、架构、libc 变体。
//!
//! Provider 需要把「官方产物命名」翻译成 `Platform`，因此这里只保留
//! 运行时所必需的最小信息，不做任何探测副作用。

use serde::{Deserialize, Serialize};

use crate::{PlatformError, RuntimeError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Os {
    Linux,
    Macos,
    Windows,
}

impl Os {
    pub fn id(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Macos => "macos",
            Self::Windows => "windows",
        }
    }

    pub fn executable_suffix(self) -> &'static str {
        match self {
            Self::Windows => ".exe",
            _ => "",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arch {
    X64,
    Arm64,
}

impl Arch {
    pub fn id(self) -> &'static str {
        match self {
            Self::X64 => "x64",
            Self::Arm64 => "arm64",
        }
    }
}

/// Linux 的 C 运行库差异会直接影响官方产物（glibc vs musl）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Libc {
    Gnu,
    Musl,
    /// macOS / Windows 没有 libc 变体。
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Platform {
    pub os: Os,
    pub arch: Arch,
    pub libc: Libc,
}

impl Platform {
    /// 探测当前运行的平台。`libc` 通过 musl 动态链接器是否存在判断。
    pub fn detect() -> Result<Self, RuntimeError> {
        let os = match std::env::consts::OS {
            "linux" => Os::Linux,
            "macos" => Os::Macos,
            "windows" => Os::Windows,
            other => return Err(PlatformError::UnsupportedOs(other.to_string()).into()),
        };
        let arch = match std::env::consts::ARCH {
            "x86_64" => Arch::X64,
            "aarch64" => Arch::Arm64,
            other => return Err(PlatformError::UnsupportedArch(other.to_string()).into()),
        };
        let libc = match os {
            Os::Linux => detect_linux_libc(),
            _ => Libc::None,
        };
        Ok(Self { os, arch, libc })
    }

    pub fn is_windows(&self) -> bool {
        self.os == Os::Windows
    }

    /// 可执行文件后缀（Windows 为 `.exe`）。
    pub fn exe_suffix(&self) -> &'static str {
        self.os.executable_suffix()
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.libc {
            Libc::None => write!(f, "{}-{}", self.os.id(), self.arch.id()),
            Libc::Gnu => write!(f, "{}-{}-gnu", self.os.id(), self.arch.id()),
            Libc::Musl => write!(f, "{}-{}-musl", self.os.id(), self.arch.id()),
        }
    }
}

/// 没有可靠的跨发行版运行时 API，这里沿用社区做法：
/// 存在 musl 的动态链接器就判定为 musl，否则按 glibc 处理。
fn detect_linux_libc() -> Libc {
    if std::path::Path::new("/lib/ld-musl-x86_64.so.1").exists()
        || std::path::Path::new("/lib/ld-musl-aarch64.so.1").exists()
    {
        return Libc::Musl;
    }
    Libc::Gnu
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_includes_libc_only_on_linux() {
        let windows = Platform {
            os: Os::Windows,
            arch: Arch::X64,
            libc: Libc::None,
        };
        assert_eq!(windows.to_string(), "windows-x64");
        assert_eq!(windows.exe_suffix(), ".exe");

        let linux = Platform {
            os: Os::Linux,
            arch: Arch::Arm64,
            libc: Libc::Musl,
        };
        assert_eq!(linux.to_string(), "linux-arm64-musl");
        assert_eq!(linux.exe_suffix(), "");
    }

    #[test]
    fn detect_returns_supported_platform() {
        let platform = Platform::detect().expect("current platform is supported");
        if platform.is_windows() {
            assert_eq!(platform.libc, Libc::None);
        }
    }
}
