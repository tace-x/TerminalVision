//! Language detection and syntax definitions for code preview.

use std::path::Path;

/// Supported programming, markup, and configuration languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    JavaScript,
    TypeScript,
    Python,
    Java,
    C,
    Cpp,
    Go,
    Php,
    Ruby,
    CSharp,
    Shell,
    Json,
    Toml,
    Yaml,
    HtmlXml,
    Css,
    Markdown,
    PlainText,
}

impl Language {
    /// Detects the language from a file path based on its extension or filename.
    pub fn from_path(path: &Path) -> Self {
        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
            let lower = file_name.to_ascii_lowercase();
            match lower.as_str() {
                "cargo.lock" | "cargo.toml" => return Self::Toml,
                "package.json" | "tsconfig.json" => return Self::Json,
                "makefile" | "dockerfile" => return Self::PlainText,
                ".bashrc" | ".zshrc" | ".profile" => return Self::Shell,
                "gemfile" | "rakefile" => return Self::Ruby,
                "composer.json" => return Self::Json,
                "go.mod" | "go.sum" => return Self::Go,
                ".env" => return Self::PlainText,
                _ => {}
            }
        }

        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            Self::from_extension(ext)
        } else {
            Self::PlainText
        }
    }

    /// Detects the language from a file extension (case-insensitive).
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_ascii_lowercase().as_str() {
            "rs" => Self::Rust,
            "js" | "jsx" | "mjs" | "cjs" => Self::JavaScript,
            "ts" | "tsx" | "mts" | "cts" => Self::TypeScript,
            "py" | "pyw" => Self::Python,
            "java" => Self::Java,
            "c" | "h" => Self::C,
            "cpp" | "hpp" | "cc" | "cxx" | "hxx" => Self::Cpp,
            "go" => Self::Go,
            "php" | "phtml" | "php3" | "php4" | "php5" | "phps" => Self::Php,
            "rb" | "rake" | "gemspec" => Self::Ruby,
            "cs" | "csx" => Self::CSharp,
            "sh" | "bash" | "zsh" => Self::Shell,
            "json" => Self::Json,
            "toml" => Self::Toml,
            "yaml" | "yml" => Self::Yaml,
            "html" | "htm" | "xml" | "svg" => Self::HtmlXml,
            "css" | "scss" | "sass" | "less" => Self::Css,
            "md" | "markdown" => Self::Markdown,
            "txt" | "conf" | "ini" | "env" | "log" | "sql" => Self::PlainText,
            _ => Self::PlainText,
        }
    }

    /// The human-readable display name of the language.
    pub fn name(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::Python => "Python",
            Self::Java => "Java",
            Self::C => "C",
            Self::Cpp => "C++",
            Self::Go => "Go",
            Self::Php => "PHP",
            Self::Ruby => "Ruby",
            Self::CSharp => "C#",
            Self::Shell => "Shell",
            Self::Json => "JSON",
            Self::Toml => "TOML",
            Self::Yaml => "YAML",
            Self::HtmlXml => "HTML/XML",
            Self::Css => "CSS",
            Self::Markdown => "Markdown",
            Self::PlainText => "Plain Text",
        }
    }

    /// Line comment prefix for the language, if any.
    pub fn line_comment_prefix(self) -> Option<&'static str> {
        match self {
            Self::Rust
            | Self::JavaScript
            | Self::TypeScript
            | Self::Java
            | Self::C
            | Self::Cpp
            | Self::Go
            | Self::Php
            | Self::CSharp => Some("//"),
            Self::Python | Self::Ruby | Self::Shell | Self::Yaml | Self::Toml => Some("#"),
            Self::PlainText | Self::Json | Self::HtmlXml | Self::Css | Self::Markdown => None,
        }
    }

    /// Checks if a word is a keyword in this language.
    pub fn is_keyword(self, word: &str) -> bool {
        match self {
            Self::Rust => matches!(
                word,
                "as" | "async"
                    | "await"
                    | "break"
                    | "const"
                    | "continue"
                    | "crate"
                    | "dyn"
                    | "else"
                    | "enum"
                    | "extern"
                    | "fn"
                    | "for"
                    | "if"
                    | "impl"
                    | "in"
                    | "let"
                    | "loop"
                    | "match"
                    | "mod"
                    | "move"
                    | "mut"
                    | "pub"
                    | "ref"
                    | "return"
                    | "self"
                    | "Self"
                    | "static"
                    | "struct"
                    | "super"
                    | "trait"
                    | "type"
                    | "unsafe"
                    | "use"
                    | "where"
                    | "while"
            ),
            Self::JavaScript | Self::TypeScript => matches!(
                word,
                "async"
                    | "await"
                    | "break"
                    | "case"
                    | "catch"
                    | "class"
                    | "const"
                    | "continue"
                    | "debugger"
                    | "default"
                    | "delete"
                    | "do"
                    | "else"
                    | "enum"
                    | "export"
                    | "extends"
                    | "finally"
                    | "for"
                    | "from"
                    | "function"
                    | "if"
                    | "implements"
                    | "import"
                    | "in"
                    | "instanceof"
                    | "interface"
                    | "let"
                    | "new"
                    | "package"
                    | "private"
                    | "protected"
                    | "public"
                    | "return"
                    | "static"
                    | "super"
                    | "switch"
                    | "this"
                    | "throw"
                    | "try"
                    | "type"
                    | "typeof"
                    | "var"
                    | "void"
                    | "while"
                    | "with"
                    | "yield"
            ),
            Self::Python => matches!(
                word,
                "and"
                    | "as"
                    | "assert"
                    | "async"
                    | "await"
                    | "break"
                    | "class"
                    | "continue"
                    | "def"
                    | "del"
                    | "elif"
                    | "else"
                    | "except"
                    | "finally"
                    | "for"
                    | "from"
                    | "global"
                    | "if"
                    | "import"
                    | "in"
                    | "is"
                    | "lambda"
                    | "nonlocal"
                    | "not"
                    | "or"
                    | "pass"
                    | "raise"
                    | "return"
                    | "try"
                    | "while"
                    | "with"
                    | "yield"
            ),
            Self::Java => matches!(
                word,
                "abstract"
                    | "assert"
                    | "break"
                    | "case"
                    | "catch"
                    | "class"
                    | "const"
                    | "continue"
                    | "default"
                    | "do"
                    | "else"
                    | "enum"
                    | "extends"
                    | "final"
                    | "finally"
                    | "for"
                    | "if"
                    | "implements"
                    | "import"
                    | "instanceof"
                    | "interface"
                    | "native"
                    | "new"
                    | "package"
                    | "private"
                    | "protected"
                    | "public"
                    | "return"
                    | "static"
                    | "strictfp"
                    | "super"
                    | "switch"
                    | "synchronized"
                    | "this"
                    | "throw"
                    | "throws"
                    | "transient"
                    | "try"
                    | "void"
                    | "volatile"
                    | "while"
            ),
            Self::C | Self::Cpp => matches!(
                word,
                "auto"
                    | "break"
                    | "case"
                    | "char"
                    | "class"
                    | "const"
                    | "continue"
                    | "default"
                    | "delete"
                    | "do"
                    | "double"
                    | "else"
                    | "enum"
                    | "extern"
                    | "float"
                    | "for"
                    | "goto"
                    | "if"
                    | "inline"
                    | "int"
                    | "long"
                    | "namespace"
                    | "new"
                    | "operator"
                    | "private"
                    | "protected"
                    | "public"
                    | "register"
                    | "return"
                    | "short"
                    | "signed"
                    | "sizeof"
                    | "static"
                    | "struct"
                    | "switch"
                    | "template"
                    | "this"
                    | "typedef"
                    | "typename"
                    | "union"
                    | "unsigned"
                    | "using"
                    | "virtual"
                    | "void"
                    | "volatile"
                    | "while"
            ),
            Self::Go => matches!(
                word,
                "break"
                    | "case"
                    | "chan"
                    | "const"
                    | "continue"
                    | "default"
                    | "defer"
                    | "else"
                    | "fallthrough"
                    | "for"
                    | "func"
                    | "go"
                    | "goto"
                    | "if"
                    | "import"
                    | "interface"
                    | "map"
                    | "package"
                    | "range"
                    | "return"
                    | "select"
                    | "struct"
                    | "switch"
                    | "type"
                    | "var"
            ),
            Self::Php => matches!(
                word,
                "abstract"
                    | "and"
                    | "array"
                    | "as"
                    | "break"
                    | "callable"
                    | "case"
                    | "catch"
                    | "class"
                    | "clone"
                    | "const"
                    | "continue"
                    | "declare"
                    | "default"
                    | "die"
                    | "do"
                    | "echo"
                    | "else"
                    | "elseif"
                    | "empty"
                    | "eval"
                    | "exit"
                    | "extends"
                    | "final"
                    | "finally"
                    | "fn"
                    | "for"
                    | "foreach"
                    | "function"
                    | "global"
                    | "goto"
                    | "if"
                    | "implements"
                    | "include"
                    | "include_once"
                    | "instanceof"
                    | "insteadof"
                    | "interface"
                    | "isset"
                    | "list"
                    | "match"
                    | "namespace"
                    | "new"
                    | "or"
                    | "print"
                    | "private"
                    | "protected"
                    | "public"
                    | "readonly"
                    | "require"
                    | "require_once"
                    | "return"
                    | "static"
                    | "switch"
                    | "throw"
                    | "trait"
                    | "try"
                    | "unset"
                    | "use"
                    | "var"
                    | "while"
                    | "xor"
                    | "yield"
            ),
            Self::Ruby => matches!(
                word,
                "alias"
                    | "and"
                    | "begin"
                    | "break"
                    | "case"
                    | "class"
                    | "def"
                    | "defined"
                    | "do"
                    | "else"
                    | "elsif"
                    | "end"
                    | "ensure"
                    | "for"
                    | "if"
                    | "in"
                    | "module"
                    | "next"
                    | "not"
                    | "or"
                    | "redo"
                    | "rescue"
                    | "retry"
                    | "return"
                    | "self"
                    | "super"
                    | "then"
                    | "undef"
                    | "unless"
                    | "until"
                    | "when"
                    | "while"
                    | "yield"
            ),
            Self::CSharp => matches!(
                word,
                "abstract"
                    | "as"
                    | "async"
                    | "await"
                    | "base"
                    | "bool"
                    | "break"
                    | "byte"
                    | "case"
                    | "catch"
                    | "char"
                    | "checked"
                    | "class"
                    | "const"
                    | "continue"
                    | "decimal"
                    | "default"
                    | "delegate"
                    | "do"
                    | "double"
                    | "else"
                    | "enum"
                    | "event"
                    | "explicit"
                    | "extern"
                    | "finally"
                    | "fixed"
                    | "float"
                    | "for"
                    | "foreach"
                    | "goto"
                    | "if"
                    | "implicit"
                    | "in"
                    | "int"
                    | "interface"
                    | "internal"
                    | "is"
                    | "lock"
                    | "long"
                    | "namespace"
                    | "new"
                    | "object"
                    | "operator"
                    | "out"
                    | "override"
                    | "params"
                    | "private"
                    | "protected"
                    | "public"
                    | "readonly"
                    | "record"
                    | "ref"
                    | "return"
                    | "sbyte"
                    | "sealed"
                    | "short"
                    | "sizeof"
                    | "stackalloc"
                    | "static"
                    | "string"
                    | "struct"
                    | "switch"
                    | "this"
                    | "throw"
                    | "try"
                    | "typeof"
                    | "uint"
                    | "ulong"
                    | "unchecked"
                    | "unsafe"
                    | "ushort"
                    | "using"
                    | "var"
                    | "virtual"
                    | "void"
                    | "volatile"
                    | "while"
                    | "yield"
            ),
            Self::Shell => matches!(
                word,
                "case"
                    | "do"
                    | "done"
                    | "elif"
                    | "else"
                    | "esac"
                    | "exit"
                    | "export"
                    | "fi"
                    | "for"
                    | "function"
                    | "if"
                    | "in"
                    | "local"
                    | "read"
                    | "return"
                    | "select"
                    | "set"
                    | "then"
                    | "time"
                    | "until"
                    | "while"
            ),
            Self::Json
            | Self::Toml
            | Self::Yaml
            | Self::HtmlXml
            | Self::Css
            | Self::Markdown
            | Self::PlainText => false,
        }
    }

    /// Checks if a word is a boolean or null literal in this language.
    pub fn is_boolean_or_null(self, word: &str) -> bool {
        match self {
            Self::Python => matches!(word, "True" | "False" | "None"),
            Self::Rust => matches!(word, "true" | "false" | "Some" | "None" | "Ok" | "Err"),
            Self::JavaScript
            | Self::TypeScript
            | Self::Java
            | Self::C
            | Self::Cpp
            | Self::CSharp => {
                matches!(word, "true" | "false" | "null" | "undefined" | "nullptr")
            }
            Self::Go => matches!(word, "true" | "false" | "nil" | "iota"),
            Self::Php => matches!(word, "true" | "false" | "null" | "TRUE" | "FALSE" | "NULL"),
            Self::Ruby => matches!(word, "true" | "false" | "nil"),
            Self::Json | Self::Yaml | Self::Toml => {
                matches!(word, "true" | "false" | "null" | "TRUE" | "FALSE")
            }
            _ => matches!(word, "true" | "false"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_detection_from_extension() {
        assert_eq!(Language::from_extension("rs"), Language::Rust);
        assert_eq!(Language::from_extension("js"), Language::JavaScript);
        assert_eq!(Language::from_extension("jsx"), Language::JavaScript);
        assert_eq!(Language::from_extension("ts"), Language::TypeScript);
        assert_eq!(Language::from_extension("tsx"), Language::TypeScript);
        assert_eq!(Language::from_extension("py"), Language::Python);
        assert_eq!(Language::from_extension("java"), Language::Java);
        assert_eq!(Language::from_extension("c"), Language::C);
        assert_eq!(Language::from_extension("h"), Language::C);
        assert_eq!(Language::from_extension("cpp"), Language::Cpp);
        assert_eq!(Language::from_extension("hpp"), Language::Cpp);
        assert_eq!(Language::from_extension("go"), Language::Go);
        assert_eq!(Language::from_extension("php"), Language::Php);
        assert_eq!(Language::from_extension("rb"), Language::Ruby);
        assert_eq!(Language::from_extension("cs"), Language::CSharp);
        assert_eq!(Language::from_extension("sh"), Language::Shell);
        assert_eq!(Language::from_extension("bash"), Language::Shell);
        assert_eq!(Language::from_extension("zsh"), Language::Shell);
        assert_eq!(Language::from_extension("json"), Language::Json);
        assert_eq!(Language::from_extension("toml"), Language::Toml);
        assert_eq!(Language::from_extension("yaml"), Language::Yaml);
        assert_eq!(Language::from_extension("yml"), Language::Yaml);
        assert_eq!(Language::from_extension("html"), Language::HtmlXml);
        assert_eq!(Language::from_extension("xml"), Language::HtmlXml);
        assert_eq!(Language::from_extension("css"), Language::Css);
        assert_eq!(Language::from_extension("md"), Language::Markdown);
        assert_eq!(Language::from_extension("txt"), Language::PlainText);
    }

    #[test]
    fn case_insensitive_extensions() {
        assert_eq!(Language::from_extension("RS"), Language::Rust);
        assert_eq!(Language::from_extension("Py"), Language::Python);
        assert_eq!(Language::from_extension("JSON"), Language::Json);
        assert_eq!(Language::from_extension("CPP"), Language::Cpp);
        assert_eq!(Language::from_extension("HTML"), Language::HtmlXml);
    }

    #[test]
    fn language_detection_from_path() {
        assert_eq!(
            Language::from_path(Path::new("src/main.rs")),
            Language::Rust
        );
        assert_eq!(Language::from_path(Path::new("Cargo.toml")), Language::Toml);
        assert_eq!(Language::from_path(Path::new(".bashrc")), Language::Shell);
        assert_eq!(
            Language::from_path(Path::new("LICENSE")),
            Language::PlainText
        );
        assert_eq!(
            Language::from_path(Path::new("unknown.xyz")),
            Language::PlainText
        );
    }

    #[test]
    fn keyword_recognition_per_language() {
        assert!(Language::Rust.is_keyword("fn"));
        assert!(Language::Rust.is_keyword("struct"));
        assert!(!Language::Rust.is_keyword("def"));

        assert!(Language::Python.is_keyword("def"));
        assert!(Language::Python.is_keyword("elif"));
        assert!(!Language::Python.is_keyword("fn"));

        assert!(Language::JavaScript.is_keyword("function"));
        assert!(Language::JavaScript.is_keyword("const"));

        assert!(Language::C.is_keyword("typedef"));
        assert!(Language::Cpp.is_keyword("template"));
    }

    #[test]
    fn boolean_and_null_recognition() {
        assert!(Language::Rust.is_boolean_or_null("true"));
        assert!(Language::Rust.is_boolean_or_null("None"));
        assert!(Language::Python.is_boolean_or_null("True"));
        assert!(Language::Python.is_boolean_or_null("None"));
        assert!(Language::JavaScript.is_boolean_or_null("null"));
        assert!(Language::JavaScript.is_boolean_or_null("undefined"));
    }
}
