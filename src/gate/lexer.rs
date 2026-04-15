use super::token::{DurationUnit, Token};

#[derive(Debug, Clone)]
pub struct LexError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "LexError at {}:{} — {}", self.line, self.col, self.message)
    }
}

pub struct Lexer {
    source: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Self {
            source: source.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();

        loop {
            self.skip_whitespace_no_newline();

            if self.pos >= self.source.len() {
                tokens.push(Token::Eof);
                break;
            }

            let ch = self.current();

            // Comments
            if ch == '/' && self.peek(1) == Some('/') {
                self.skip_line_comment();
                continue;
            }
            if ch == '/' && self.peek(1) == Some('*') {
                self.skip_block_comment()?;
                continue;
            }

            // Newlines
            if ch == '\n' {
                tokens.push(Token::Newline);
                self.advance();
                continue;
            }

            // String literals
            if ch == '"' {
                tokens.push(self.lex_string()?);
                continue;
            }

            // Regex literals /pattern/
            if ch == '/' {
                tokens.push(self.lex_regex()?);
                continue;
            }

            // Numbers (and duration: 30s, 5m, 1h, 7d)
            if ch.is_ascii_digit() {
                tokens.push(self.lex_number_or_duration()?);
                continue;
            }

            // Identifiers and keywords
            if ch.is_alphabetic() || ch == '_' {
                tokens.push(self.lex_ident());
                continue;
            }

            // Operators and delimiters
            let tok = match ch {
                '{' => { self.advance(); Token::LBrace }
                '}' => { self.advance(); Token::RBrace }
                '(' => { self.advance(); Token::LParen }
                ')' => { self.advance(); Token::RParen }
                '[' => { self.advance(); Token::LBracket }
                ']' => { self.advance(); Token::RBracket }
                ',' => { self.advance(); Token::Comma }
                ':' => { self.advance(); Token::Colon }
                '.' => { self.advance(); Token::Dot }
                '+' => { self.advance(); Token::Plus }
                '-' => { self.advance(); Token::Minus }
                '*' => { self.advance(); Token::Star }
                '=' => {
                    self.advance();
                    if self.current() == '=' {
                        self.advance();
                        Token::Eq
                    } else {
                        Token::Assign
                    }
                }
                '!' => {
                    self.advance();
                    if self.current() == '=' {
                        self.advance();
                        Token::NotEq
                    } else {
                        Token::Bang
                    }
                }
                '<' => {
                    self.advance();
                    if self.current() == '=' {
                        self.advance();
                        Token::LtEq
                    } else {
                        Token::Lt
                    }
                }
                '>' => {
                    self.advance();
                    if self.current() == '=' {
                        self.advance();
                        Token::GtEq
                    } else {
                        Token::Gt
                    }
                }
                '&' if self.peek(1) == Some('&') => {
                    self.advance(); self.advance();
                    Token::And
                }
                '|' if self.peek(1) == Some('|') => {
                    self.advance(); self.advance();
                    Token::Or
                }
                _ => {
                    return Err(LexError {
                        message: format!("unexpected character '{}'", ch),
                        line: self.line,
                        col: self.col,
                    });
                }
            };

            tokens.push(tok);
        }

        Ok(tokens)
    }

    // --- String "hello {var} world" with interpolation markers ---
    fn lex_string(&mut self) -> Result<Token, LexError> {
        self.advance(); // consume opening "
        let mut s = String::new();

        loop {
            if self.pos >= self.source.len() {
                return Err(LexError {
                    message: "unterminated string literal".to_string(),
                    line: self.line,
                    col: self.col,
                });
            }
            let ch = self.current();
            if ch == '"' {
                self.advance();
                break;
            }
            if ch == '\\' {
                self.advance();
                let escaped = match self.current() {
                    'n'  => '\n',
                    't'  => '\t',
                    '"'  => '"',
                    '\\' => '\\',
                    other => other,
                };
                s.push(escaped);
                self.advance();
                continue;
            }
            s.push(ch);
            self.advance();
        }

        Ok(Token::StringLit(s))
    }

    // --- Regex /pattern/ ---
    fn lex_regex(&mut self) -> Result<Token, LexError> {
        self.advance(); // consume /
        let mut pattern = String::new();

        loop {
            if self.pos >= self.source.len() || self.current() == '\n' {
                return Err(LexError {
                    message: "unterminated regex literal".to_string(),
                    line: self.line,
                    col: self.col,
                });
            }
            let ch = self.current();
            if ch == '/' {
                self.advance();
                break;
            }
            if ch == '\\' {
                self.advance();
                pattern.push('\\');
                pattern.push(self.current());
                self.advance();
                continue;
            }
            pattern.push(ch);
            self.advance();
        }

        Ok(Token::RegexLit(pattern))
    }

    // --- Numbers and durations: 42, 3.14, 30s, 5m, 1h, 7d ---
    fn lex_number_or_duration(&mut self) -> Result<Token, LexError> {
        let mut num = String::new();

        while self.pos < self.source.len() && (self.current().is_ascii_digit() || self.current() == '.') {
            num.push(self.current());
            self.advance();
        }

        let value: f64 = num.parse().map_err(|_| LexError {
            message: format!("invalid number '{}'", num),
            line: self.line,
            col: self.col,
        })?;

        // Check for duration suffix
        if self.pos < self.source.len() {
            let unit = match self.current() {
                's' => { self.advance(); Some(DurationUnit::Seconds) }
                'm' => { self.advance(); Some(DurationUnit::Minutes) }
                'h' => { self.advance(); Some(DurationUnit::Hours) }
                'd' => { self.advance(); Some(DurationUnit::Days) }
                _   => None,
            };
            if let Some(u) = unit {
                return Ok(Token::DurationLit(value, u));
            }
        }

        Ok(Token::NumberLit(value))
    }

    // --- Identifiers and keywords ---
    fn lex_ident(&mut self) -> Token {
        let mut ident = String::new();

        while self.pos < self.source.len() && (self.current().is_alphanumeric() || self.current() == '_') {
            ident.push(self.current());
            self.advance();
        }

        Token::from_ident(&ident)
    }

    // --- Skip // comments ---
    fn skip_line_comment(&mut self) {
        while self.pos < self.source.len() && self.current() != '\n' {
            self.advance();
        }
    }

    // --- Skip /* */ comments ---
    fn skip_block_comment(&mut self) -> Result<(), LexError> {
        self.advance(); self.advance(); // consume /*
        loop {
            if self.pos + 1 >= self.source.len() {
                return Err(LexError {
                    message: "unterminated block comment".to_string(),
                    line: self.line,
                    col: self.col,
                });
            }
            if self.current() == '*' && self.peek(1) == Some('/') {
                self.advance(); self.advance();
                return Ok(());
            }
            self.advance();
        }
    }

    // --- Skip spaces and tabs (not newlines) ---
    fn skip_whitespace_no_newline(&mut self) {
        while self.pos < self.source.len() {
            match self.current() {
                ' ' | '\t' | '\r' => self.advance(),
                _ => break,
            }
        }
    }

    fn current(&self) -> char {
        self.source.get(self.pos).copied().unwrap_or('\0')
    }

    fn peek(&self, offset: usize) -> Option<char> {
        self.source.get(self.pos + offset).copied()
    }

    fn advance(&mut self) {
        if self.pos < self.source.len() {
            if self.current() == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
            self.pos += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_workflow() {
        let src = r#"workflow deploy(message) {
    save(message)
    sync()
}"#;
        let mut lexer = Lexer::new(src);
        let tokens = lexer.tokenize().unwrap();
        assert!(tokens.contains(&Token::Workflow));
        assert!(tokens.contains(&Token::Ident("deploy".to_string())));
        assert!(tokens.contains(&Token::LBrace));
    }

    #[test]
    fn test_duration() {
        let mut lexer = Lexer::new("30s");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0], Token::DurationLit(30.0, DurationUnit::Seconds));
    }

    #[test]
    fn test_string_interpolation() {
        let mut lexer = Lexer::new(r#""Release {version} complete""#);
        let tokens = lexer.tokenize().unwrap();
        assert!(matches!(&tokens[0], Token::StringLit(s) if s.contains("{version}")));
    }

    #[test]
    fn test_comment_skipped() {
        let mut lexer = Lexer::new("// this is a comment\nworkflow");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0], Token::Newline);
        assert_eq!(tokens[1], Token::Workflow);
    }

    #[test]
    fn test_regex() {
        let mut lexer = Lexer::new("/feat:.*/");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0], Token::RegexLit("feat:.*".to_string()));
    }
}
