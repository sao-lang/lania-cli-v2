//! 命名推导（§4.5.4）：从一个 `name` 推出所有占位符变体。
//!
//! 这是「一个名字生成 7 个文件」的基础。刻意**不引入 `inflector` 依赖**：
//! 生成器场景下错误的单复数会立刻体现在文件名/类名上，用户一眼就能发现并改名，
//! 为此多一个依赖不划算。内置规则覆盖 `-s` / `-es` / `-ies` / 约 30 个不规则词。

/// 一个名字的全部变体（键名与模板占位符一致）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameVariants {
    /// 用户原始输入（已做 trim）。
    pub raw: String,
    /// `{name}`：保持用户书写形态的 kebab。
    pub name: String,
    /// `{Name}` / `{name_pascal}`
    pub name_pascal: String,
    /// `{name_camel}`
    pub name_camel: String,
    /// `{name_kebab}`
    pub name_kebab: String,
    /// `{name_snake}`
    pub name_snake: String,
    /// `{name_singular}`
    pub name_singular: String,
    /// `{NameSingular}`
    pub name_singular_pascal: String,
}

impl NameVariants {
    /// 用于模板渲染与路径替换的占位符表。
    pub fn placeholders(&self) -> Vec<(&'static str, &str)> {
        vec![
            ("name", &self.name),
            ("Name", &self.name_pascal),
            ("name_pascal", &self.name_pascal),
            ("name_camel", &self.name_camel),
            ("name_kebab", &self.name_kebab),
            ("name_snake", &self.name_snake),
            ("name_singular", &self.name_singular),
            ("NameSingular", &self.name_singular_pascal),
        ]
    }
}

/// 从用户输入推导所有变体。
///
/// `users` → `Users` / `user`；`user-profile` → `UserProfile` / `userProfile` …
pub fn name_variants(input: &str) -> NameVariants {
    let mut words = split_words(input);
    if words.is_empty() {
        words = vec!["index".to_string()];
    }
    let pascal = words
        .iter()
        .map(|word| capitalize(word))
        .collect::<Vec<_>>()
        .join("");
    let camel = {
        let mut out = String::new();
        for (index, word) in words.iter().enumerate() {
            if index == 0 {
                out.push_str(word);
            } else {
                out.push_str(&capitalize(word));
            }
        }
        out
    };
    let kebab = words.join("-");
    let snake = words.join("_");
    let (singular_pascal, singular_snake) = singularize(&pascal, &words);
    NameVariants {
        raw: input.trim().to_string(),
        name: kebab.clone(),
        name_pascal: pascal,
        name_camel: camel,
        name_kebab: kebab,
        name_snake: snake,
        name_singular: singular_snake,
        name_singular_pascal: singular_pascal,
    }
}

/// 把任意书写形态拆成小写单词：`UserProfile` / `user_profile` / `userProfile` / `user-profile`。
fn split_words(input: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut previous_lower = false;
    for ch in input.trim().chars() {
        if ch == '-' || ch == '_' || ch == '.' || ch == '/' || ch == ' ' {
            if !current.is_empty() {
                words.push(current.to_lowercase());
                current.clear();
            }
            previous_lower = false;
            continue;
        }
        if ch.is_uppercase() && previous_lower && !current.is_empty() {
            words.push(current.to_lowercase());
            current.clear();
        }
        previous_lower = ch.is_lowercase() || ch.is_numeric();
        current.push(ch);
    }
    if !current.is_empty() {
        words.push(current.to_lowercase());
    }
    words
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// 不参与单复数转换的词：技术缩写与不可数名词。
const SINGULAR_EXEMPT: &[&str] = &["dto", "id", "api", "url", "http", "sql", "css", "html"];

/// 不规则复数表（复数 → 单数）。
const IRREGULAR: &[(&str, &str)] = &[
    ("people", "person"),
    ("children", "child"),
    ("men", "man"),
    ("women", "woman"),
    ("teeth", "tooth"),
    ("feet", "foot"),
    ("mice", "mouse"),
    ("geese", "goose"),
    ("data", "datum"),
    ("criteria", "criterion"),
    ("indices", "index"),
    ("matrices", "matrix"),
    ("vertices", "vertex"),
    ("analyses", "analysis"),
    ("bases", "basis"),
    ("crises", "crisis"),
    ("diagnoses", "diagnosis"),
    ("theses", "thesis"),
    ("addresses", "address"),
    ("statuses", "status"),
    ("responses", "response"),
    ("databases", "database"),
    ("licenses", "license"),
    ("categories", "category"),
    ("companies", "company"),
    ("properties", "property"),
    ("entries", "entry"),
    ("stories", "story"),
    ("users", "user"),
];

/// 返回 `(Pascal 单数, snake 单数)`。
///
/// 只处理最后一个单词：`user-profiles` → `user-profile`，避免把限定词也一起变形。
fn singularize(pascal: &str, words: &[String]) -> (String, String) {
    let Some(last) = words.last() else {
        return (pascal.to_string(), String::new());
    };
    let singular_last = singular_word(last);
    let prefix_pascal = words[..words.len() - 1]
        .iter()
        .map(|word| capitalize(word))
        .collect::<Vec<_>>()
        .join("");
    let prefix_snake = words[..words.len() - 1].join("_");
    let pascal_singular = format!("{prefix_pascal}{}", capitalize(&singular_last));
    let snake_singular = if prefix_snake.is_empty() {
        singular_last
    } else {
        format!("{prefix_snake}_{singular_last}")
    };
    (pascal_singular, snake_singular)
}

fn singular_word(word: &str) -> String {
    if SINGULAR_EXEMPT.contains(&word) {
        return word.to_string();
    }
    if let Some((_, singular)) = IRREGULAR.iter().find(|(plural, _)| *plural == word) {
        return (*singular).to_string();
    }
    let len = word.chars().count();
    if len > 3 && word.ends_with("ies") {
        return format!("{}y", &word[..len - 3]);
    }
    if len > 3 && (word.ends_with("ches") || word.ends_with("shes") || word.ends_with("sses")) {
        return word[..len - 2].to_string();
    }
    if len > 2 && (word.ends_with("xes") || word.ends_with("zes") || word.ends_with("ses")) {
        return word[..len - 2].to_string();
    }
    // `bus` / `analysis` / `status` 这类以 `s` 结尾但不是复数的词，交给不规则表或原样保留。
    if len > 1 && word.ends_with('s') && !word.ends_with("ss") && !word.ends_with("us") {
        return word[..len - 1].to_string();
    }
    word.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_variants_for_plural_input() {
        let variants = name_variants("users");
        assert_eq!(variants.name_pascal, "Users");
        assert_eq!(variants.name_camel, "users");
        assert_eq!(variants.name_kebab, "users");
        assert_eq!(variants.name_snake, "users");
        assert_eq!(variants.name_singular, "user");
        assert_eq!(variants.name_singular_pascal, "User");
    }

    #[test]
    fn derives_variants_for_kebab_input() {
        let variants = name_variants("user-profile");
        assert_eq!(variants.name_pascal, "UserProfile");
        assert_eq!(variants.name_camel, "userProfile");
        assert_eq!(variants.name_kebab, "user-profile");
        assert_eq!(variants.name_snake, "user_profile");
        assert_eq!(variants.name_singular, "user_profile");
        assert_eq!(variants.name_singular_pascal, "UserProfile");
    }

    #[test]
    fn splits_mixed_case_input() {
        let variants = name_variants("UserProfile");
        assert_eq!(variants.name_kebab, "user-profile");
        assert_eq!(variants.name_snake, "user_profile");
    }

    #[test]
    fn singularizes_common_irregular_forms() {
        assert_eq!(name_variants("categories").name_singular, "category");
        assert_eq!(name_variants("people").name_singular, "person");
        assert_eq!(name_variants("addresses").name_singular, "address");
        assert_eq!(name_variants("boxes").name_singular, "box");
        assert_eq!(name_variants("status").name_singular, "status");
        assert_eq!(name_variants("user-dtos").name_singular, "user_dto");
    }

    #[test]
    fn empty_input_falls_back_to_index() {
        assert_eq!(name_variants("  ").name_kebab, "index");
    }
}
