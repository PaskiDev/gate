/// A complete Gate source file
#[derive(Debug, Clone)]
pub struct Program {
    pub items: Vec<Item>,
}

/// Top-level items in a Gate file
#[derive(Debug, Clone)]
pub enum Item {
    Import(ImportDecl),
    Workflow(WorkflowDecl),
    Struct(StructDecl),
    Impl(ImplDecl),
    Enum(EnumDecl),
    Policy(PolicyDecl),
}

/// `policy commits { ... }` — declarative rules consumed by external tools
/// (torii commit-scan today; CI gates later). Pure data: parser only
/// validates the shape; semantic checks (regex compile, etc.) happen in the
/// consumer.
#[derive(Debug, Clone)]
pub struct PolicyDecl {
    pub kind: PolicyKind,
    pub rules: Vec<PolicyRule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyKind {
    Commits,
}

/// One line inside a `policy commits { ... }` block. Fields hold raw token
/// payloads; consumer compiles the regex / interprets the limit.
#[derive(Debug, Clone)]
pub enum PolicyRule {
    /// `forbid trailer /regex/`
    ForbidTrailer(String),
    /// `require trailer /regex/`
    RequireTrailer(String),
    /// `forbid subject /regex/`
    ForbidSubject(String),
    /// `author email matches /regex/`
    AuthorEmailMatches(String),
    /// `subject max_length 72`
    SubjectMaxLength(usize),
    /// `subject min_length 8`
    SubjectMinLength(usize),
    /// `conventional_commits required`
    ConventionalCommitsRequired,
}

/// import "path/to/file.gate"
#[derive(Debug, Clone)]
pub struct ImportDecl {
    pub path: String,
}

/// workflow name(param1, param2 = default) { ... }
#[derive(Debug, Clone)]
pub struct WorkflowDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub body: Block,
}

/// struct Name { field: Type = default, ... }
#[derive(Debug, Clone)]
pub struct StructDecl {
    pub name: String,
    pub fields: Vec<StructField>,
}

#[derive(Debug, Clone)]
pub struct StructField {
    pub name: String,
    pub ty: TypeExpr,
    pub default: Option<Expr>,
}

/// impl Name { fn method() { ... } }
#[derive(Debug, Clone)]
pub struct ImplDecl {
    pub name: String,
    pub methods: Vec<FnDecl>,
}

/// fn name(params) { body }
#[derive(Debug, Clone)]
pub struct FnDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub body: Block,
}

/// enum Name { variant1, variant2 }
#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<String>,
}

/// Function/workflow parameter
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Option<TypeExpr>,
    pub default: Option<Expr>,
}

/// A block of statements { ... }
#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub on_error: Option<Box<Block>>,
    pub on_timeout: Option<Box<Block>>,
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Stmt {
    /// name = expr
    Assign(AssignStmt),
    /// expr (function call, await, etc.)
    Expr(Expr),
    /// return expr
    Return(Option<Expr>),
    /// if cond { } else { }
    If(IfStmt),
    /// for name in expr { }
    For(ForStmt),
}

#[derive(Debug, Clone)]
pub struct AssignStmt {
    pub target: String,
    pub ty: Option<TypeExpr>,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct IfStmt {
    pub condition: Expr,
    pub then_block: Block,
    pub else_block: Option<Block>,
}

#[derive(Debug, Clone)]
pub struct ForStmt {
    pub var: String,
    pub iterable: Expr,
    pub body: Block,
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Expr {
    /// "hello {var}"
    StringLit(String),
    /// 42, 3.14
    NumberLit(f64),
    /// true / false
    BoolLit(bool),
    /// null
    Null,
    /// /pattern/
    RegexLit(String),
    /// 30s, 5m, 1h, 7d
    DurationLit(f64, DurationUnit),
    /// identifier
    Ident(String),
    /// [a, b, c]
    List(Vec<Expr>),
    /// {key: value}
    Map(Vec<(String, Expr)>),
    /// StructName { field: value }
    StructInit(String, Vec<(String, Expr)>),
    /// func(args)  or  obj.method(args)
    Call(CallExpr),
    /// obj.field
    Member(Box<Expr>, String),
    /// async expr
    Async(Box<Expr>),
    /// await expr  /  await expr timeout duration
    Await(Box<Expr>, Option<Box<Expr>>),
    /// await all(futures) timeout duration
    AwaitAll(Box<Expr>, Option<Box<Expr>>),
    /// binary op: a + b, a == b, a && b …
    BinOp(Box<Expr>, BinOp, Box<Expr>),
    /// unary op: !a
    UnaryOp(UnaryOp, Box<Expr>),
}

#[derive(Debug, Clone)]
pub struct CallExpr {
    pub callee: Box<Expr>,
    pub args: Vec<Arg>,
}

/// Positional or named argument: value  /  name: value
#[derive(Debug, Clone)]
pub enum Arg {
    Positional(Expr),
    Named(String, Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add, Sub, Mul, Div,
    Eq, NotEq, Lt, Gt, LtEq, GtEq,
    And, Or,
}

#[derive(Debug, Clone)]
pub enum UnaryOp {
    Not,
    Neg,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DurationUnit {
    Seconds,
    Minutes,
    Hours,
    Days,
}

// ---------------------------------------------------------------------------
// Type expressions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum TypeExpr {
    Named(String),      // string, number, bool, MyStruct …
    List(Box<TypeExpr>),  // list<T>  (future)
    Optional(Box<TypeExpr>), // T?  (future)
}
