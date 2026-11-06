//! 可执行脚本的运行时静态表。
//!
//! 表里只描述“怎么执行”，不负责探测本机是否安装。新增运行时只需要：

use crate::{PrepareKind, RuntimeCandidate, RuntimeSpec};

const NODE_PACKAGE_MANAGERS: &[&str] = &["npm", "pnpm", "yarn", "bun"];
const PYTHON_PACKAGE_MANAGERS: &[&str] = &["pip", "uv", "poetry"];

static RUNTIMES: &[RuntimeSpec] = &[
    RuntimeSpec {
        id: "javascript",
        display_name: "JavaScript",
        extensions: &["js", "mjs", "cjs"],
        candidates: &[
            RuntimeCandidate {
                program: "node",
                args: &[],
            },
            RuntimeCandidate {
                program: "bun",
                args: &[],
            },
        ],
        prepare: None,
        package_managers: NODE_PACKAGE_MANAGERS,
    },
    RuntimeSpec {
        id: "typescript",
        display_name: "TypeScript",
        extensions: &["ts", "tsx", "jsx"],
        candidates: &[
            RuntimeCandidate {
                program: "tsx",
                args: &[],
            },
            RuntimeCandidate {
                program: "bun",
                args: &[],
            },
            RuntimeCandidate {
                program: "ts-node",
                args: &[],
            },
        ],
        prepare: None,
        package_managers: NODE_PACKAGE_MANAGERS,
    },
    RuntimeSpec {
        id: "python",
        display_name: "Python",
        extensions: &["py"],
        candidates: &[
            RuntimeCandidate {
                program: "python3",
                args: &[],
            },
            RuntimeCandidate {
                program: "python",
                args: &[],
            },
        ],
        prepare: None,
        package_managers: PYTHON_PACKAGE_MANAGERS,
    },
    RuntimeSpec {
        id: "sh",
        display_name: "sh",
        extensions: &["sh"],
        candidates: &[
            RuntimeCandidate {
                program: "sh",
                args: &[],
            },
            RuntimeCandidate {
                program: "bash",
                args: &[],
            },
        ],
        prepare: None,
        package_managers: &[],
    },
    RuntimeSpec {
        id: "bash",
        display_name: "bash",
        extensions: &["bash"],
        candidates: &[
            RuntimeCandidate {
                program: "bash",
                args: &[],
            },
            RuntimeCandidate {
                program: "sh",
                args: &[],
            },
        ],
        prepare: None,
        package_managers: &[],
    },
    RuntimeSpec {
        id: "zsh",
        display_name: "zsh",
        extensions: &["zsh"],
        candidates: &[
            RuntimeCandidate {
                program: "zsh",
                args: &[],
            },
            RuntimeCandidate {
                program: "bash",
                args: &[],
            },
            RuntimeCandidate {
                program: "sh",
                args: &[],
            },
        ],
        prepare: None,
        package_managers: &[],
    },
    RuntimeSpec {
        id: "ruby",
        display_name: "Ruby",
        extensions: &["rb"],
        candidates: &[RuntimeCandidate {
            program: "ruby",
            args: &[],
        }],
        prepare: None,
        package_managers: &["bundler"],
    },
    RuntimeSpec {
        id: "php",
        display_name: "PHP",
        extensions: &["php"],
        candidates: &[RuntimeCandidate {
            program: "php",
            args: &[],
        }],
        prepare: None,
        package_managers: &["composer"],
    },
    RuntimeSpec {
        id: "go",
        display_name: "Go",
        extensions: &["go"],
        candidates: &[RuntimeCandidate {
            program: "go",
            args: &["run"],
        }],
        prepare: None,
        package_managers: &["go"],
    },
    RuntimeSpec {
        id: "java",
        display_name: "Java",
        extensions: &["java"],
        candidates: &[RuntimeCandidate {
            program: "java",
            args: &[],
        }],
        prepare: None,
        package_managers: &[],
    },
    RuntimeSpec {
        id: "rust",
        display_name: "Rust",
        extensions: &["rs"],
        candidates: &[RuntimeCandidate {
            program: "rustc",
            args: &[],
        }],
        prepare: Some(PrepareKind::Rustc),
        package_managers: &["cargo"],
    },
    RuntimeSpec {
        id: "c",
        display_name: "C",
        extensions: &["c"],
        candidates: &[
            RuntimeCandidate {
                program: "cc",
                args: &[],
            },
            RuntimeCandidate {
                program: "clang",
                args: &[],
            },
            RuntimeCandidate {
                program: "gcc",
                args: &[],
            },
        ],
        prepare: Some(PrepareKind::Cc),
        package_managers: &[],
    },
    RuntimeSpec {
        id: "lua",
        display_name: "Lua",
        extensions: &["lua"],
        candidates: &[RuntimeCandidate {
            program: "lua",
            args: &[],
        }],
        prepare: None,
        package_managers: &[],
    },
    RuntimeSpec {
        id: "dart",
        display_name: "Dart",
        extensions: &["dart"],
        candidates: &[RuntimeCandidate {
            program: "dart",
            args: &["run"],
        }],
        prepare: None,
        package_managers: &["pub"],
    },
    RuntimeSpec {
        id: "nim",
        display_name: "Nim",
        extensions: &["nim"],
        candidates: &[RuntimeCandidate {
            program: "nim",
            args: &["r"],
        }],
        prepare: None,
        package_managers: &[],
    },
    RuntimeSpec {
        id: "zig",
        display_name: "Zig",
        extensions: &["zig"],
        candidates: &[RuntimeCandidate {
            program: "zig",
            args: &["run"],
        }],
        prepare: None,
        package_managers: &[],
    },
    RuntimeSpec {
        id: "kotlin",
        display_name: "Kotlin",
        extensions: &["kt", "kts"],
        candidates: &[
            RuntimeCandidate {
                program: "kotlin",
                args: &[],
            },
            RuntimeCandidate {
                program: "kotlinc",
                args: &["-script"],
            },
        ],
        prepare: None,
        package_managers: &[],
    },
    RuntimeSpec {
        id: "swift",
        display_name: "Swift",
        extensions: &["swift"],
        candidates: &[RuntimeCandidate {
            program: "swift",
            args: &[],
        }],
        prepare: None,
        package_managers: &[],
    },
];

pub(crate) fn runtimes() -> &'static [RuntimeSpec] {
    RUNTIMES
}

pub(crate) fn find_by_language(language: &str) -> Option<&'static RuntimeSpec> {
    RUNTIMES.iter().find(|spec| spec.id == language)
}

pub(crate) fn find_by_extension(extension: &str) -> Option<&'static RuntimeSpec> {
    RUNTIMES
        .iter()
        .find(|spec| spec.extensions.iter().any(|item| *item == extension))
}
