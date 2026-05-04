/// All tokens the Gate lexer can produce
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // --- Literals ---
    StringLit(String),      // "hello"
    NumberLit(f64),         // 42, 3.14
    BoolLit(bool),          // true, false
    NullLit,                // null
    RegexLit(String),       // /pattern/
    DurationLit(f64, DurationUnit), // 30s, 5m, 1h, 7d

    // --- Identifiers & keywords ---
    Ident(String),

    // Keywords
    Workflow,
    Struct,
    Impl,
    Enum,
    Import,
    For,
    In,
    If,
    Else,
    Return,
    Async,
    Await,
    All,
    Timeout,
    Fn,
    OnError,
    OnTimeout,
    Self_,

    // Policy DSL keywords (used inside `policy <kind> { ... }` blocks)
    Policy,
    Commits,
    Forbid,
    Require,
    Trailer,
    Subject,
    Author,
    Email,
    Matches,
    MaxLength,
    MinLength,
    ConventionalCommits,
    Required,

    // Type keywords
    TString,
    TNumber,
    TBool,
    TList,
    TMap,
    TVersion,
    TPath,
    TUrl,
    TRegex,
    TBytes,
    TDate,
    TDatetime,
    TDuration,
    TFuture,
    TChannel,

    // --- Operators ---
    Assign,       // =
    Eq,           // ==
    NotEq,        // !=
    Lt,           // <
    Gt,           // >
    LtEq,         // <=
    GtEq,         // >=
    Plus,         // +
    Minus,        // -
    Star,         // *
    Slash,        // /
    Bang,         // !
    And,          // &&
    Or,           // ||

    // --- Delimiters ---
    LBrace,       // {
    RBrace,       // }
    LParen,       // (
    RParen,       // )
    LBracket,     // [
    RBracket,     // ]
    Comma,        // ,
    Colon,        // :
    Dot,          // .
    Newline,

    // --- Special ---
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DurationUnit {
    Seconds,
    Minutes,
    Hours,
    Days,
}

impl Token {
    /// Map identifier strings to keywords
    pub fn from_ident(s: &str) -> Token {
        match s {
            "workflow"   => Token::Workflow,
            "struct"     => Token::Struct,
            "impl"       => Token::Impl,
            "enum"       => Token::Enum,
            "import"     => Token::Import,
            "for"        => Token::For,
            "in"         => Token::In,
            "if"         => Token::If,
            "else"       => Token::Else,
            "return"     => Token::Return,
            "async"      => Token::Async,
            "await"      => Token::Await,
            "all"        => Token::All,
            "timeout"    => Token::Timeout,
            "fn"         => Token::Fn,
            "on_error"   => Token::OnError,
            "on_timeout" => Token::OnTimeout,
            "self"       => Token::Self_,
            "policy"               => Token::Policy,
            "commits"              => Token::Commits,
            "forbid"               => Token::Forbid,
            "require"              => Token::Require,
            "trailer"              => Token::Trailer,
            "subject"              => Token::Subject,
            "author"               => Token::Author,
            "email"                => Token::Email,
            "matches"              => Token::Matches,
            "max_length"           => Token::MaxLength,
            "min_length"           => Token::MinLength,
            "conventional_commits" => Token::ConventionalCommits,
            "required"             => Token::Required,
            "true"       => Token::BoolLit(true),
            "false"      => Token::BoolLit(false),
            "null"       => Token::NullLit,
            // Type keywords
            "string"     => Token::TString,
            "number"     => Token::TNumber,
            "bool"       => Token::TBool,
            "list"       => Token::TList,
            "map"        => Token::TMap,
            "version"    => Token::TVersion,
            "path"       => Token::TPath,
            "url"        => Token::TUrl,
            "regex"      => Token::TRegex,
            "bytes"      => Token::TBytes,
            "date"       => Token::TDate,
            "datetime"   => Token::TDatetime,
            "duration"   => Token::TDuration,
            "future"     => Token::TFuture,
            "channel"    => Token::TChannel,
            _            => Token::Ident(s.to_string()),
        }
    }
}
