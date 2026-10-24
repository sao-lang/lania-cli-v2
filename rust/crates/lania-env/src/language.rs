//! 支持管理的语言 / 运行时。
//!
//! 每种语言的探测方式、候选管理器、附属工具、项目版本文件都集中在这张表的
//! 一个分支里，新增语言只需要：
//! 1. 在枚举里加一个变体
//! 2. 在 `spec()` 里补齐该语言的一行元数据
//! 3. 在 `parse_id()` 里加上别名

use serde::Serialize;

use crate::ManagerKind;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    Node,
    Deno,
    Bun,
    Python,
    Rust,
    Go,
    Zig,
    Dart,
    Kotlin,
    Java,
    Scala,
    Groovy,
    C,
    Cpp,
    CSharp,
    FSharp,
    Swift,
    Ruby,
    PHP,
    Perl,
    Lua,
    Elixir,
    Erlang,
    Haskell,
    Julia,
    R,
    Nim,
    Crystal,
    OCaml,
    Clojure,
    MoonBit,
    Cangjie,
    ArkTs,
    Solidity,
    Fortran,
    D,
    V,
    Gleam,
    Haxe,
    Elm,
}

/// 一种语言的全部静态元数据。
struct LanguageSpec {
    id: &'static str,
    name: &'static str,
    /// 用于 `--version` 探测的可执行文件名（按顺序尝试）
    probes: &'static [&'static str],
    /// 候选版本管理器（顺序即偏好）
    managers: &'static [ManagerKind],
    /// 附带展示的包管理器（含依赖/构建工具）；不包含 lint、格式化、LSP、编译器。
    companions: &'static [&'static str],
    /// 项目级版本锁定文件
    version_files: &'static [&'static str],
}

macro_rules! managers {
    ($($m:ident),* $(,)?) => {
        &[$(ManagerKind::$m),*]
    };
}

fn spec(language: Language) -> LanguageSpec {
    match language {
        Language::Node => LanguageSpec {
            id: "node",
            name: "Node.js",
            probes: &["node"],
            managers: managers![Nvm, Fnm, Volta, N, Asdf, Mise, Scoop, Winget, Brew, Apt],
            companions: &["npm", "pnpm", "yarn", "corepack"],
            version_files: &[".nvmrc", ".node-version"],
        },
        Language::Deno => LanguageSpec {
            id: "deno",
            name: "Deno",
            probes: &["deno"],
            managers: managers![Denoup, Asdf, Mise, Scoop, Winget, Brew],
            companions: &[],
            version_files: &[],
        },
        Language::Bun => LanguageSpec {
            id: "bun",
            name: "Bun",
            probes: &["bun"],
            managers: managers![Bunup, Asdf, Mise, Scoop, Winget, Brew],
            companions: &[],
            version_files: &[".bun-version"],
        },
        Language::Python => LanguageSpec {
            id: "python",
            name: "Python",
            probes: &["python3", "python"],
            managers: managers![Pyenv, Uv, Asdf, Mise, Scoop, Winget, Brew, Apt],
            companions: &["pip", "pipx", "poetry"],
            version_files: &[".python-version"],
        },
        Language::Rust => LanguageSpec {
            id: "rust",
            name: "Rust",
            probes: &["rustc"],
            managers: managers![Rustup],
            companions: &["cargo"],
            version_files: &["rust-toolchain.toml", "rust-toolchain"],
        },
        Language::Go => LanguageSpec {
            id: "go",
            name: "Go",
            probes: &["go"],
            managers: managers![GoInstall, Asdf, Mise, Scoop, Winget, Brew, Apt],
            companions: &[],
            version_files: &["go.mod"],
        },
        Language::Zig => LanguageSpec {
            id: "zig",
            name: "Zig",
            probes: &["zig"],
            managers: managers![ZigVm, Asdf, Mise, Scoop, Winget, Brew],
            companions: &[],
            version_files: &[".zigversion"],
        },
        Language::Dart => LanguageSpec {
            id: "dart",
            name: "Dart",
            probes: &["dart"],
            managers: managers![Fvm, Asdf, Scoop, Winget, Brew, Apt],
            companions: &["pub"],
            version_files: &[".fvmrc", "pubspec.yaml"],
        },
        Language::Kotlin => LanguageSpec {
            id: "kotlin",
            name: "Kotlin",
            probes: &["kotlinc", "kotlin"],
            managers: managers![Sdkman, Asdf, Mise, Scoop, Winget, Brew],
            companions: &["gradle"],
            version_files: &[".sdkmanrc"],
        },
        Language::Java => LanguageSpec {
            id: "java",
            name: "Java",
            probes: &["java"],
            managers: managers![Sdkman, Asdf, Mise, Scoop, Winget, Brew, Apt],
            companions: &["mvn", "gradle"],
            version_files: &[".sdkmanrc", ".java-version"],
        },
        Language::Scala => LanguageSpec {
            id: "scala",
            name: "Scala",
            probes: &["scala", "scalac"],
            managers: managers![Sdkman, Coursier, Asdf, Mise, Brew, Apt],
            companions: &["sbt"],
            version_files: &[".sdkmanrc"],
        },
        Language::Groovy => LanguageSpec {
            id: "groovy",
            name: "Groovy",
            probes: &["groovy"],
            managers: managers![Sdkman, Asdf, Mise, Scoop, Brew, Apt],
            companions: &[],
            version_files: &[".sdkmanrc"],
        },
        Language::C => LanguageSpec {
            id: "c",
            name: "C",
            probes: &["gcc", "clang", "cc"],
            managers: managers![Scoop, Winget, Brew, Apt],
            companions: &[],
            version_files: &[],
        },
        Language::Cpp => LanguageSpec {
            id: "cpp",
            name: "C++",
            probes: &["g++", "clang++", "c++"],
            managers: managers![Scoop, Winget, Brew, Apt],
            companions: &[],
            version_files: &[],
        },
        Language::CSharp => LanguageSpec {
            id: "csharp",
            name: "C#",
            probes: &["dotnet"],
            managers: managers![Dotnet, Asdf, Mise, Scoop, Winget, Brew, Apt],
            companions: &["nuget"],
            version_files: &["global.json"],
        },
        Language::FSharp => LanguageSpec {
            id: "fsharp",
            name: "F#",
            probes: &["dotnet"],
            managers: managers![Dotnet, Asdf, Mise, Scoop, Winget, Brew],
            companions: &[],
            version_files: &["global.json"],
        },
        Language::Swift => LanguageSpec {
            id: "swift",
            name: "Swift",
            probes: &["swift"],
            managers: managers![Swiftenv, Asdf, Mise, Scoop, Winget, Brew],
            companions: &[],
            version_files: &[".swift-version"],
        },
        Language::Ruby => LanguageSpec {
            id: "ruby",
            name: "Ruby",
            probes: &["ruby"],
            managers: managers![Rbenv, Rvm, Asdf, Mise, Scoop, Winget, Brew, Apt],
            companions: &["gem", "bundler"],
            version_files: &[".ruby-version"],
        },
        Language::PHP => LanguageSpec {
            id: "php",
            name: "PHP",
            probes: &["php"],
            managers: managers![Phpenv, Asdf, Mise, Scoop, Winget, Brew, Apt],
            companions: &["composer"],
            version_files: &[".php-version"],
        },
        Language::Perl => LanguageSpec {
            id: "perl",
            name: "Perl",
            probes: &["perl"],
            managers: managers![Plenv, Asdf, Mise, Scoop, Brew, Apt],
            companions: &["cpan", "cpanm"],
            version_files: &[".perl-version"],
        },
        Language::Lua => LanguageSpec {
            id: "lua",
            name: "Lua",
            probes: &["lua"],
            managers: managers![Luaver, Asdf, Mise, Scoop, Winget, Brew, Apt],
            companions: &["luarocks"],
            version_files: &[".lua-version"],
        },
        Language::Elixir => LanguageSpec {
            id: "elixir",
            name: "Elixir",
            probes: &["elixir"],
            managers: managers![Asdf, Mise, Scoop, Winget, Brew, Apt],
            companions: &["mix"],
            version_files: &[".tool-versions"],
        },
        Language::Erlang => LanguageSpec {
            id: "erlang",
            name: "Erlang",
            probes: &["erl"],
            managers: managers![KerL, Asdf, Mise, Scoop, Winget, Brew, Apt],
            companions: &["rebar3"],
            version_files: &[".tool-versions"],
        },
        Language::Haskell => LanguageSpec {
            id: "haskell",
            name: "Haskell",
            probes: &["ghc"],
            managers: managers![GhcUp, Asdf, Mise, Brew, Apt],
            companions: &["cabal", "stack"],
            version_files: &[".tool-versions"],
        },
        Language::Julia => LanguageSpec {
            id: "julia",
            name: "Julia",
            probes: &["julia"],
            managers: managers![Juliaup, Asdf, Mise, Scoop, Winget, Brew, Apt],
            companions: &[],
            version_files: &[".julia-version"],
        },
        Language::R => LanguageSpec {
            id: "r",
            name: "R",
            probes: &["R", "Rscript"],
            managers: managers![Rig, Asdf, Mise, Scoop, Winget, Brew, Apt],
            companions: &[],
            version_files: &[".Rversion"],
        },
        Language::Nim => LanguageSpec {
            id: "nim",
            name: "Nim",
            probes: &["nim"],
            managers: managers![Choosenim, Asdf, Mise, Scoop, Brew],
            companions: &["nimble"],
            version_files: &[],
        },
        Language::Crystal => LanguageSpec {
            id: "crystal",
            name: "Crystal",
            probes: &["crystal"],
            managers: managers![Asdf, Mise, Brew, Apt],
            companions: &["shards"],
            version_files: &[".crystal-version"],
        },
        Language::OCaml => LanguageSpec {
            id: "ocaml",
            name: "OCaml",
            probes: &["ocaml"],
            managers: managers![Opam, Asdf, Mise, Brew, Apt],
            companions: &["opam"],
            version_files: &[".ocaml-version"],
        },
        Language::Clojure => LanguageSpec {
            id: "clojure",
            name: "Clojure",
            probes: &["clojure", "clj"],
            managers: managers![Cljup, Sdkman, Asdf, Mise, Brew, Apt],
            companions: &["lein"],
            version_files: &[".tool-versions"],
        },
        Language::MoonBit => LanguageSpec {
            id: "moonbit",
            name: "MoonBit",
            probes: &["moon"],
            // MoonBit 官方工具链自带版本管理（`moon version --all`、
            // `moon toolchain`），没有第三方管理器可对接。
            managers: managers![Moonup],
            companions: &[],
            version_files: &["moon.mod.json"],
        },
        Language::Cangjie => LanguageSpec {
            id: "cangjie",
            name: "Cangjie",
            probes: &["cjc", "cjpm"],
            // 华为仓颉：官方随 DevEco / Cangjie SDK 分发，无社区管理器。
            managers: managers![Cangjieup],
            companions: &["cjpm"],
            version_files: &["cjpm.toml"],
        },
        Language::ArkTs => LanguageSpec {
            id: "arkts",
            name: "ArkTS",
            probes: &["ark", "ohpm", "hvigorw"],
            // 鸿蒙（HarmonyOS）应用开发：SDK 随 DevEco Studio 管理。
            managers: managers![Deveco],
            companions: &["ohpm"],
            version_files: &["oh-package.json5", "build-profile.json5"],
        },
        Language::Solidity => LanguageSpec {
            id: "solidity",
            name: "Solidity",
            probes: &["solc", "forge"],
            managers: managers![Foundryup, Asdf, Mise, Brew, Apt],
            companions: &["forge"],
            version_files: &["foundry.toml"],
        },
        Language::Fortran => LanguageSpec {
            id: "fortran",
            name: "Fortran",
            probes: &["gfortran", "flang", "ifort"],
            managers: managers![Asdf, Mise, Brew, Apt],
            companions: &["fpm"],
            version_files: &[".fpm-version"],
        },
        Language::D => LanguageSpec {
            id: "d",
            name: "D",
            probes: &["dmd", "ldc2", "gdc"],
            managers: managers![Asdf, Mise, Brew, Apt],
            companions: &["dub"],
            version_files: &["dub.json", "dub.sdl"],
        },
        Language::V => LanguageSpec {
            id: "v",
            name: "V",
            probes: &["v"],
            managers: managers![Vup, Asdf, Mise, Brew],
            companions: &["vpm"],
            version_files: &["v.mod"],
        },
        Language::Gleam => LanguageSpec {
            id: "gleam",
            name: "Gleam",
            probes: &["gleam"],
            managers: managers![Asdf, Mise, Brew, Apt],
            companions: &["rebar3"],
            version_files: &["gleam.toml"],
        },
        Language::Haxe => LanguageSpec {
            id: "haxe",
            name: "Haxe",
            probes: &["haxe"],
            managers: managers![Haxelihx, Asdf, Mise, Brew, Apt],
            companions: &["haxelib"],
            version_files: &["haxelib.json"],
        },
        Language::Elm => LanguageSpec {
            id: "elm",
            name: "Elm",
            probes: &["elm"],
            managers: managers![Asdf, Mise, Brew, Apt],
            companions: &[],
            version_files: &["elm.json"],
        },
    }
}

impl Language {
    pub fn id(self) -> &'static str {
        spec(self).id
    }

    pub fn display_name(self) -> &'static str {
        spec(self).name
    }

    /// 语言/运行时的一句话简介（TUI 详情面板使用）。
    pub fn summary(self, zh: bool) -> &'static str {
        match (self, zh) {
            (Self::Node, false) => "JavaScript runtime and package ecosystem",
            (Self::Node, true) => "JavaScript 运行时与包生态",
            (Self::Deno, false) => "Secure TypeScript/JavaScript runtime",
            (Self::Deno, true) => "安全的 TypeScript/JavaScript 运行时",
            (Self::Bun, false) => "All-in-one JavaScript runtime and toolkit",
            (Self::Bun, true) => "一体化 JavaScript 运行时与工具链",
            (Self::Python, false) => "General-purpose scripting and data ecosystem",
            (Self::Python, true) => "通用脚本语言与数据科学生态",
            (Self::Rust, false) => "Systems language with Cargo tooling",
            (Self::Rust, true) => "系统级语言与 Cargo 工具链",
            (Self::Go, false) => "Concurrency-first compiled language",
            (Self::Go, true) => "并发优先的编译型语言",
            (Self::Zig, false) => "Low-level systems toolchain",
            (Self::Zig, true) => "底层系统编程工具链",
            (Self::Dart, false) => "UI-focused language with pub packages",
            (Self::Dart, true) => "面向 UI 的语言与 pub 包管理",
            (Self::Kotlin, false) => "JVM and multiplatform language",
            (Self::Kotlin, true) => "JVM 与多平台开发语言",
            (Self::Java, false) => "JVM enterprise platform",
            (Self::Java, true) => "JVM 企业级开发平台",
            (Self::Scala, false) => "Functional language on the JVM",
            (Self::Scala, true) => "JVM 上的函数式编程语言",
            (Self::Groovy, false) => "Scripting language on the JVM",
            (Self::Groovy, true) => "JVM 上的脚本语言",
            (Self::C, false) => "C compiler and libc toolchain",
            (Self::C, true) => "C 编译器与 libc 工具链",
            (Self::Cpp, false) => "C++ compiler and build chain",
            (Self::Cpp, true) => "C++ 编译器与构建链",
            (Self::CSharp, false) => "Language on the .NET platform",
            (Self::CSharp, true) => ".NET 平台开发语言",
            (Self::FSharp, false) => "Functional language on .NET",
            (Self::FSharp, true) => ".NET 平台函数式语言",
            (Self::Swift, false) => "Apple platform development language",
            (Self::Swift, true) => "Apple 平台开发语言",
            (Self::Ruby, false) => "Dynamic scripting language",
            (Self::Ruby, true) => "动态脚本语言",
            (Self::PHP, false) => "Web scripting language",
            (Self::PHP, true) => "Web 脚本语言",
            (Self::Perl, false) => "Text processing language",
            (Self::Perl, true) => "文本处理脚本语言",
            (Self::Lua, false) => "Embedded scripting language",
            (Self::Lua, true) => "嵌入式脚本语言",
            (Self::Elixir, false) => "Functional language on the BEAM",
            (Self::Elixir, true) => "BEAM 平台函数式语言",
            (Self::Erlang, false) => "Concurrent language on the BEAM",
            (Self::Erlang, true) => "BEAM 高并发语言",
            (Self::Haskell, false) => "Purely functional language",
            (Self::Haskell, true) => "纯函数式编程语言",
            (Self::Julia, false) => "Scientific computing language",
            (Self::Julia, true) => "科学计算语言",
            (Self::R, false) => "Statistics and data analysis",
            (Self::R, true) => "统计与数据分析语言",
            (Self::Nim, false) => "Compiled systems language",
            (Self::Nim, true) => "编译型系统编程语言",
            (Self::Crystal, false) => "Ruby-like compiled language",
            (Self::Crystal, true) => "类 Ruby 的编译型语言",
            (Self::OCaml, false) => "Functional language with opam",
            (Self::OCaml, true) => "函数式语言与 opam 生态",
            (Self::Clojure, false) => "Lisp dialect on the JVM",
            (Self::Clojure, true) => "JVM 上的 Lisp 方言",
            (Self::MoonBit, false) => "Cloud-native language toolchain",
            (Self::MoonBit, true) => "云原生语言工具链",
            (Self::Cangjie, false) => "HarmonyOS native development language",
            (Self::Cangjie, true) => "鸿蒙原生开发语言",
            (Self::ArkTs, false) => "HarmonyOS application language",
            (Self::ArkTs, true) => "鸿蒙应用开发语言",
            (Self::Solidity, false) => "Smart contracts for the EVM",
            (Self::Solidity, true) => "EVM 智能合约语言",
            (Self::Fortran, false) => "Scientific computing language",
            (Self::Fortran, true) => "科学计算语言",
            (Self::D, false) => "Systems language with dub",
            (Self::D, true) => "系统编程语言与 dub 生态",
            (Self::V, false) => "Simple compiled language",
            (Self::V, true) => "简洁的编译型语言",
            (Self::Gleam, false) => "Typed language on the BEAM",
            (Self::Gleam, true) => "BEAM 平台类型化语言",
            (Self::Haxe, false) => "Cross-platform compiled language",
            (Self::Haxe, true) => "跨平台编译语言",
            (Self::Elm, false) => "Functional frontend language",
            (Self::Elm, true) => "前端函数式语言",
        }
    }

    /// 该语言通常由哪些管理器负责安装。顺序即偏好顺序。
    pub fn managers(self) -> Vec<ManagerKind> {
        spec(self).managers.to_vec()
    }

    /// 用于 `--version` 探测的可执行文件名（按顺序尝试）。
    pub fn probe_binaries(self) -> Vec<&'static str> {
        spec(self).probes.to_vec()
    }

    /// 包管理器 / 构建工具（附带在语言之后作为独立工具展示）。
    pub fn companions(self) -> Vec<&'static str> {
        spec(self).companions.to_vec()
    }

    /// 项目级版本锁定文件名（优先匹配）。
    pub fn version_files(self) -> Vec<&'static str> {
        spec(self).version_files.to_vec()
    }

    /// 直接读取并返回版本文件中声明的版本（若文件存在且内容可解析）。
    pub fn read_version_file(self, cwd: &std::path::Path) -> Option<(String, String)> {
        for name in self.version_files() {
            let path = cwd.join(name);
            if !path.is_file() {
                continue;
            }
            let content = std::fs::read_to_string(&path).ok()?;
            if let Some(version) = crate::extract_version(self, name, &content) {
                return Some((name.to_string(), version));
            }
        }
        None
    }

    pub fn parse_id(value: &str) -> Option<Self> {
        let value = value.trim().to_ascii_lowercase();
        match value.as_str() {
            "node" | "nodejs" | "js" | "javascript" => Some(Self::Node),
            "deno" => Some(Self::Deno),
            "bun" => Some(Self::Bun),
            "python" | "py" | "python3" => Some(Self::Python),
            "rust" | "rustc" | "cargo" | "rustlang" => Some(Self::Rust),
            "go" | "golang" => Some(Self::Go),
            "zig" => Some(Self::Zig),
            "dart" | "flutter" => Some(Self::Dart),
            "kotlin" | "kt" | "kts" => Some(Self::Kotlin),
            "java" | "jdk" | "jvm" | "openjdk" => Some(Self::Java),
            "scala" => Some(Self::Scala),
            "groovy" => Some(Self::Groovy),
            "c" => Some(Self::C),
            "cpp" | "c++" | "cxx" | "g++" => Some(Self::Cpp),
            "csharp" | "c#" | "cs" | "dotnet" => Some(Self::CSharp),
            "fsharp" | "f#" | "fs" => Some(Self::FSharp),
            "swift" => Some(Self::Swift),
            "ruby" | "rb" => Some(Self::Ruby),
            "php" => Some(Self::PHP),
            "perl" | "pl" => Some(Self::Perl),
            "lua" => Some(Self::Lua),
            "elixir" | "ex" => Some(Self::Elixir),
            "erlang" | "erl" => Some(Self::Erlang),
            "haskell" | "hs" | "ghc" => Some(Self::Haskell),
            "julia" | "jl" => Some(Self::Julia),
            "r" | "rscript" => Some(Self::R),
            "nim" => Some(Self::Nim),
            "crystal" | "cr" => Some(Self::Crystal),
            "ocaml" | "ml" => Some(Self::OCaml),
            "clojure" | "clj" => Some(Self::Clojure),
            "moonbit" | "moon" | "mb" => Some(Self::MoonBit),
            "cangjie" | "cj" | "仓颉" => Some(Self::Cangjie),
            "arkts" | "ark" | "harmony" | "harmonyos" | "鸿蒙" => Some(Self::ArkTs),
            "solidity" | "sol" => Some(Self::Solidity),
            "fortran" | "f90" | "gfortran" => Some(Self::Fortran),
            "d" | "dlang" => Some(Self::D),
            "v" | "vlang" => Some(Self::V),
            "gleam" => Some(Self::Gleam),
            "haxe" => Some(Self::Haxe),
            "elm" => Some(Self::Elm),
            _ => None,
        }
    }

    pub fn all() -> Vec<Self> {
        vec![
            Self::Node,
            Self::Deno,
            Self::Bun,
            Self::Python,
            Self::Rust,
            Self::Go,
            Self::Zig,
            Self::Dart,
            Self::Java,
            Self::Kotlin,
            Self::Scala,
            Self::Groovy,
            Self::C,
            Self::Cpp,
            Self::CSharp,
            Self::FSharp,
            Self::Swift,
            Self::Ruby,
            Self::PHP,
            Self::Perl,
            Self::Lua,
            Self::Elixir,
            Self::Erlang,
            Self::Haskell,
            Self::Julia,
            Self::R,
            Self::Nim,
            Self::Crystal,
            Self::OCaml,
            Self::Clojure,
            Self::MoonBit,
            Self::Cangjie,
            Self::ArkTs,
            Self::Solidity,
            Self::Fortran,
            Self::D,
            Self::V,
            Self::Gleam,
            Self::Haxe,
            Self::Elm,
        ]
    }
}
