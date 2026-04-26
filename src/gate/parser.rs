use super::ast::*;
use super::token::{Token, DurationUnit as TokDuration};

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub pos: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "ParseError at token {}: {}", self.pos, self.message)
    }
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        // Filter out newlines — we don't use them for grammar
        let tokens = tokens.into_iter().filter(|t| t != &Token::Newline).collect();
        Self { tokens, pos: 0 }
    }

    pub fn parse(&mut self) -> Result<Program, ParseError> {
        let mut items = Vec::new();

        while !self.is_eof() {
            items.push(self.parse_item()?);
        }

        Ok(Program { items })
    }

    // -----------------------------------------------------------------------
    // Items
    // -----------------------------------------------------------------------

    fn parse_item(&mut self) -> Result<Item, ParseError> {
        match self.current() {
            Token::Import   => Ok(Item::Import(self.parse_import()?)),
            Token::Workflow => Ok(Item::Workflow(self.parse_workflow()?)),
            Token::Struct   => Ok(Item::Struct(self.parse_struct()?)),
            Token::Impl     => Ok(Item::Impl(self.parse_impl()?)),
            Token::Enum     => Ok(Item::Enum(self.parse_enum()?)),
            other => Err(self.error(format!("unexpected token at top level: {:?}", other))),
        }
    }

    fn parse_import(&mut self) -> Result<ImportDecl, ParseError> {
        self.expect(Token::Import)?;
        let path = self.expect_string()?;
        Ok(ImportDecl { path })
    }

    fn parse_workflow(&mut self) -> Result<WorkflowDecl, ParseError> {
        self.expect(Token::Workflow)?;
        let name = self.expect_ident()?;
        let params = self.parse_param_list()?;
        let body = self.parse_block()?;
        Ok(WorkflowDecl { name, params, body })
    }

    fn parse_struct(&mut self) -> Result<StructDecl, ParseError> {
        self.expect(Token::Struct)?;
        let name = self.expect_ident()?;
        self.expect(Token::LBrace)?;

        let mut fields = Vec::new();
        while self.current() != Token::RBrace && !self.is_eof() {
            let fname = self.expect_ident()?;
            self.expect(Token::Colon)?;
            let ty = self.parse_type()?;
            let default = if self.eat(Token::Assign) {
                Some(self.parse_expr()?)
            } else {
                None
            };
            fields.push(StructField { name: fname, ty, default });
            self.eat(Token::Comma);
        }

        self.expect(Token::RBrace)?;
        Ok(StructDecl { name, fields })
    }

    fn parse_impl(&mut self) -> Result<ImplDecl, ParseError> {
        self.expect(Token::Impl)?;
        let name = self.expect_ident()?;
        self.expect(Token::LBrace)?;

        let mut methods = Vec::new();
        while self.current() != Token::RBrace && !self.is_eof() {
            self.expect(Token::Fn)?;
            let mname = self.expect_ident()?;
            let params = self.parse_param_list()?;
            let body = self.parse_block()?;
            methods.push(FnDecl { name: mname, params, body });
        }

        self.expect(Token::RBrace)?;
        Ok(ImplDecl { name, methods })
    }

    fn parse_enum(&mut self) -> Result<EnumDecl, ParseError> {
        self.expect(Token::Enum)?;
        let name = self.expect_ident()?;
        self.expect(Token::LBrace)?;

        let mut variants = Vec::new();
        while self.current() != Token::RBrace && !self.is_eof() {
            variants.push(self.expect_ident()?);
            self.eat(Token::Comma);
        }

        self.expect(Token::RBrace)?;
        Ok(EnumDecl { name, variants })
    }

    // -----------------------------------------------------------------------
    // Parameters
    // -----------------------------------------------------------------------

    fn parse_param_list(&mut self) -> Result<Vec<Param>, ParseError> {
        self.expect(Token::LParen)?;
        let mut params = Vec::new();

        while self.current() != Token::RParen && !self.is_eof() {
            let name = self.expect_ident()?;
            let ty = if self.eat(Token::Colon) {
                Some(self.parse_type()?)
            } else {
                None
            };
            let default = if self.eat(Token::Assign) {
                Some(self.parse_expr()?)
            } else {
                None
            };
            params.push(Param { name, ty, default });
            self.eat(Token::Comma);
        }

        self.expect(Token::RParen)?;
        Ok(params)
    }

    // -----------------------------------------------------------------------
    // Block
    // -----------------------------------------------------------------------

    fn parse_block(&mut self) -> Result<Block, ParseError> {
        self.expect(Token::LBrace)?;
        let mut stmts = Vec::new();
        let mut on_error = None;
        let mut on_timeout = None;

        while self.current() != Token::RBrace && !self.is_eof() {
            match self.current() {
                Token::OnError => {
                    self.advance();
                    on_error = Some(Box::new(self.parse_block()?));
                }
                Token::OnTimeout => {
                    self.advance();
                    on_timeout = Some(Box::new(self.parse_block()?));
                }
                _ => stmts.push(self.parse_stmt()?),
            }
        }

        self.expect(Token::RBrace)?;
        Ok(Block { stmts, on_error, on_timeout })
    }

    // -----------------------------------------------------------------------
    // Statements
    // -----------------------------------------------------------------------

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        match self.current() {
            Token::Return => {
                self.advance();
                let val = if self.current() != Token::RBrace {
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                Ok(Stmt::Return(val))
            }
            Token::If => Ok(Stmt::If(self.parse_if()?)),
            Token::For => Ok(Stmt::For(self.parse_for()?)),
            Token::Ident(_) if self.peek_is_assign() => Ok(Stmt::Assign(self.parse_assign()?)),
            _ => Ok(Stmt::Expr(self.parse_expr()?)),
        }
    }

    fn parse_assign(&mut self) -> Result<AssignStmt, ParseError> {
        let target = self.expect_ident()?;
        let ty = if self.eat(Token::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(Token::Assign)?;
        let value = self.parse_expr()?;
        Ok(AssignStmt { target, ty, value })
    }

    fn parse_if(&mut self) -> Result<IfStmt, ParseError> {
        self.expect(Token::If)?;
        let condition = self.parse_expr()?;
        let then_block = self.parse_block()?;
        let else_block = if self.eat(Token::Else) {
            Some(self.parse_block()?)
        } else {
            None
        };
        Ok(IfStmt { condition, then_block, else_block })
    }

    fn parse_for(&mut self) -> Result<ForStmt, ParseError> {
        self.expect(Token::For)?;
        let var = self.expect_ident()?;
        self.expect(Token::In)?;
        let iterable = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(ForStmt { var, iterable, body })
    }

    // -----------------------------------------------------------------------
    // Expressions
    // -----------------------------------------------------------------------

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and()?;
        while self.eat(Token::Or) {
            let right = self.parse_and()?;
            left = Expr::BinOp(Box::new(left), BinOp::Or, Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_equality()?;
        while self.eat(Token::And) {
            let right = self.parse_equality()?;
            left = Expr::BinOp(Box::new(left), BinOp::And, Box::new(right));
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_comparison()?;
        loop {
            let op = match self.current() {
                Token::Eq    => BinOp::Eq,
                Token::NotEq => BinOp::NotEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_comparison()?;
            left = Expr::BinOp(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_addition()?;
        loop {
            let op = match self.current() {
                Token::Lt   => BinOp::Lt,
                Token::Gt   => BinOp::Gt,
                Token::LtEq => BinOp::LtEq,
                Token::GtEq => BinOp::GtEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_addition()?;
            left = Expr::BinOp(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn parse_addition(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.current() {
                Token::Plus  => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            left = Expr::BinOp(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        if self.eat(Token::Bang) {
            return Ok(Expr::UnaryOp(UnaryOp::Not, Box::new(self.parse_unary()?)));
        }
        if self.eat(Token::Minus) {
            return Ok(Expr::UnaryOp(UnaryOp::Neg, Box::new(self.parse_unary()?)));
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;

        loop {
            if self.eat(Token::Dot) {
                let field = self.expect_ident()?;
                // method call?
                if self.current() == Token::LParen {
                    let args = self.parse_arg_list()?;
                    expr = Expr::Call(CallExpr {
                        callee: Box::new(Expr::Member(Box::new(expr), field)),
                        args,
                    });
                } else {
                    expr = Expr::Member(Box::new(expr), field);
                }
            } else if self.current() == Token::LParen {
                // bare call
                let args = self.parse_arg_list()?;
                expr = Expr::Call(CallExpr {
                    callee: Box::new(expr),
                    args,
                });
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match self.current().clone() {
            Token::StringLit(s) => { self.advance(); Ok(Expr::StringLit(s)) }
            Token::NumberLit(n) => { self.advance(); Ok(Expr::NumberLit(n)) }
            Token::BoolLit(b)   => { self.advance(); Ok(Expr::BoolLit(b)) }
            Token::NullLit      => { self.advance(); Ok(Expr::Null) }
            Token::RegexLit(r)  => { self.advance(); Ok(Expr::RegexLit(r)) }
            Token::DurationLit(v, u) => {
                self.advance();
                Ok(Expr::DurationLit(v, convert_duration(u)))
            }

            // async expr
            Token::Async => {
                self.advance();
                let inner = self.parse_expr()?;
                Ok(Expr::Async(Box::new(inner)))
            }

            // await expr  /  await all(...)  with optional timeout
            Token::Await => {
                self.advance();
                if self.eat(Token::All) {
                    let args = self.parse_arg_list()?;
                    let futures = args.into_iter().next()
                        .map(|a| match a { Arg::Positional(e) => e, Arg::Named(_, e) => e })
                        .unwrap_or(Expr::Null);
                    let timeout = self.parse_timeout()?;
                    Ok(Expr::AwaitAll(Box::new(futures), timeout.map(Box::new)))
                } else {
                    let inner = self.parse_expr()?;
                    let timeout = self.parse_timeout()?;
                    Ok(Expr::Await(Box::new(inner), timeout.map(Box::new)))
                }
            }

            // list [a, b, c]
            Token::LBracket => {
                self.advance();
                let mut items = Vec::new();
                while self.current() != Token::RBracket && !self.is_eof() {
                    items.push(self.parse_expr()?);
                    self.eat(Token::Comma);
                }
                self.expect(Token::RBracket)?;
                Ok(Expr::List(items))
            }

            // map / struct init {k: v}
            Token::LBrace => {
                self.advance();
                let mut pairs = Vec::new();
                while self.current() != Token::RBrace && !self.is_eof() {
                    let key = self.expect_ident()?;
                    self.expect(Token::Colon)?;
                    let val = self.parse_expr()?;
                    pairs.push((key, val));
                    self.eat(Token::Comma);
                }
                self.expect(Token::RBrace)?;
                Ok(Expr::Map(pairs))
            }

            // grouped (expr)
            Token::LParen => {
                self.advance();
                let inner = self.parse_expr()?;
                self.expect(Token::RParen)?;
                Ok(inner)
            }

            Token::Ident(name) => {
                self.advance();
                // StructName { ... } initialiser — only if name starts with uppercase
                if self.current() == Token::LBrace
                    && name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
                {
                    self.advance();
                    let mut fields = Vec::new();
                    while self.current() != Token::RBrace && !self.is_eof() {
                        let fname = self.expect_ident()?;
                        self.expect(Token::Colon)?;
                        let fval = self.parse_expr()?;
                        fields.push((fname, fval));
                        self.eat(Token::Comma);
                    }
                    self.expect(Token::RBrace)?;
                    Ok(Expr::StructInit(name, fields))
                } else {
                    Ok(Expr::Ident(name))
                }
            }

            other => Err(self.error(format!("unexpected token in expression: {:?}", other))),
        }
    }

    fn parse_arg_list(&mut self) -> Result<Vec<Arg>, ParseError> {
        self.expect(Token::LParen)?;
        let mut args = Vec::new();

        while self.current() != Token::RParen && !self.is_eof() {
            // named arg: name: value — name can be an identifier OR a few keywords
            // that are also valid parameter names (all, timeout, in).
            if self.peek_next() == Token::Colon {
                if let Some(name) = arg_name_from_token(&self.current()) {
                    self.advance(); // name token
                    self.advance(); // colon
                    let val = self.parse_expr()?;
                    args.push(Arg::Named(name, val));
                    self.eat(Token::Comma);
                    continue;
                }
            }
            args.push(Arg::Positional(self.parse_expr()?));
            self.eat(Token::Comma);
        }

        self.expect(Token::RParen)?;
        Ok(args)
    }

    fn parse_timeout(&mut self) -> Result<Option<Expr>, ParseError> {
        if self.eat(Token::Timeout) {
            Ok(Some(self.parse_expr()?))
        } else {
            Ok(None)
        }
    }

    // -----------------------------------------------------------------------
    // Types
    // -----------------------------------------------------------------------

    fn parse_type(&mut self) -> Result<TypeExpr, ParseError> {
        let name = self.expect_ident()?;
        Ok(TypeExpr::Named(name))
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn current(&self) -> Token {
        self.tokens.get(self.pos).cloned().unwrap_or(Token::Eof)
    }

    fn peek_next(&self) -> Token {
        self.tokens.get(self.pos + 1).cloned().unwrap_or(Token::Eof)
    }

    fn advance(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }

    fn is_eof(&self) -> bool {
        matches!(self.current(), Token::Eof)
    }

    fn eat(&mut self, tok: Token) -> bool {
        if self.current() == tok {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, tok: Token) -> Result<(), ParseError> {
        if self.current() == tok {
            self.advance();
            Ok(())
        } else {
            Err(self.error(format!("expected {:?}, got {:?}", tok, self.current())))
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        // Accept both Ident and type keyword tokens as identifiers
        let name = match self.current() {
            Token::Ident(s) => s,
            Token::TString   => "string".to_string(),
            Token::TNumber   => "number".to_string(),
            Token::TBool     => "bool".to_string(),
            Token::TList     => "list".to_string(),
            Token::TMap      => "map".to_string(),
            Token::TVersion  => "version".to_string(),
            Token::TPath     => "path".to_string(),
            Token::TUrl      => "url".to_string(),
            Token::TRegex    => "regex".to_string(),
            Token::TBytes    => "bytes".to_string(),
            Token::TDate     => "date".to_string(),
            Token::TDatetime => "datetime".to_string(),
            Token::TDuration => "duration".to_string(),
            Token::TFuture   => "future".to_string(),
            Token::TChannel  => "channel".to_string(),
            other => return Err(self.error(format!("expected identifier, got {:?}", other))),
        };
        self.advance();
        Ok(name)
    }

    fn expect_string(&mut self) -> Result<String, ParseError> {
        if let Token::StringLit(s) = self.current() {
            self.advance();
            Ok(s)
        } else {
            Err(self.error(format!("expected string, got {:?}", self.current())))
        }
    }

    /// Returns true if the current token is an Ident followed by = (not ==)
    fn peek_is_assign(&self) -> bool {
        matches!(self.tokens.get(self.pos + 1), Some(Token::Assign))
            || (matches!(self.tokens.get(self.pos + 1), Some(Token::Colon))
                && matches!(self.tokens.get(self.pos + 2), Some(Token::Ident(_) | Token::TString | Token::TNumber | Token::TBool | Token::TList | Token::TMap | Token::TDuration | Token::TDate | Token::TDatetime | Token::TFuture | Token::TChannel)))
    }

    fn error(&self, message: String) -> ParseError {
        ParseError { message, pos: self.pos }
    }
}

/// If the current token can plausibly serve as a named-argument key, return
/// the spelled-out name. Identifiers always qualify; a few keywords (`all`,
/// `timeout`, `in`) are common parameter names too and are allowed in this
/// position.
fn arg_name_from_token(tok: &Token) -> Option<String> {
    match tok {
        Token::Ident(n) => Some(n.clone()),
        Token::All      => Some("all".to_string()),
        Token::Timeout  => Some("timeout".to_string()),
        Token::In       => Some("in".to_string()),
        _ => None,
    }
}

fn convert_duration(u: TokDuration) -> DurationUnit {
    match u {
        TokDuration::Seconds => DurationUnit::Seconds,
        TokDuration::Minutes => DurationUnit::Minutes,
        TokDuration::Hours   => DurationUnit::Hours,
        TokDuration::Days    => DurationUnit::Days,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate::lexer::Lexer;

    fn parse(src: &str) -> Program {
        let mut lexer = Lexer::new(src);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        parser.parse().unwrap()
    }

    #[test]
    fn test_simple_workflow() {
        let prog = parse(r#"
workflow deploy(message, env = "prod") {
    save(message)
    sync(push: true)
}
"#);
        assert_eq!(prog.items.len(), 1);
        if let Item::Workflow(w) = &prog.items[0] {
            assert_eq!(w.name, "deploy");
            assert_eq!(w.params.len(), 2);
            assert_eq!(w.body.stmts.len(), 2);
        } else {
            panic!("expected workflow");
        }
    }

    #[test]
    fn test_enum() {
        let prog = parse(r#"
enum Platform {
    github,
    gitlab,
    codeberg
}
"#);
        if let Item::Enum(e) = &prog.items[0] {
            assert_eq!(e.variants.len(), 3);
        } else {
            panic!("expected enum");
        }
    }

    #[test]
    fn test_for_loop() {
        let prog = parse(r#"
workflow sync_all() {
    for platform in platforms {
        sync(push: true)
    }
}
"#);
        if let Item::Workflow(w) = &prog.items[0] {
            assert!(matches!(w.body.stmts[0], Stmt::For(_)));
        } else {
            panic!("expected workflow");
        }
    }

    #[test]
    fn test_if_else() {
        let prog = parse(r#"
workflow check(env) {
    if env == "prod" {
        confirm("Deploy to production?")
    } else {
        sync()
    }
}
"#);
        if let Item::Workflow(w) = &prog.items[0] {
            assert!(matches!(w.body.stmts[0], Stmt::If(_)));
        } else {
            panic!("expected workflow");
        }
    }

    #[test]
    fn test_on_error_block() {
        let prog = parse(r#"
workflow deploy() {
    sync(push: true)
    on_error {
        snapshot.restore("before-deploy")
    }
}
"#);
        if let Item::Workflow(w) = &prog.items[0] {
            assert!(w.body.on_error.is_some());
        } else {
            panic!("expected workflow");
        }
    }

    #[test]
    fn test_async_await() {
        let prog = parse(r#"
workflow parallel() {
    f = async sync(push: true)
    await f timeout 30s
}
"#);
        if let Item::Workflow(w) = &prog.items[0] {
            assert_eq!(w.body.stmts.len(), 2);
        } else {
            panic!("expected workflow");
        }
    }
}
