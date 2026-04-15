use std::collections::HashMap;
use std::fmt;
use super::ast::*;

// ---------------------------------------------------------------------------
// Runtime values
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Value {
    String(String),
    Number(f64),
    Bool(bool),
    Null,
    Duration(f64, DurationUnit),
    List(Vec<Value>),
    Map(HashMap<String, Value>),
    StructInstance(String, HashMap<String, Value>),
    EnumVariant(String, String),   // EnumName, VariantName
    Workflow(String),              // reference by name
    Future(Box<Value>),            // wraps a value (sync impl for now)
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::String(s)        => write!(f, "{}", s),
            Value::Number(n)        => {
                if n.fract() == 0.0 { write!(f, "{}", *n as i64) }
                else { write!(f, "{}", n) }
            }
            Value::Bool(b)          => write!(f, "{}", b),
            Value::Null             => write!(f, "null"),
            Value::Duration(v, u)   => write!(f, "{}{}", v, match u {
                DurationUnit::Seconds => "s",
                DurationUnit::Minutes => "m",
                DurationUnit::Hours   => "h",
                DurationUnit::Days    => "d",
            }),
            Value::List(items)      => {
                let s: Vec<String> = items.iter().map(|v| v.to_string()).collect();
                write!(f, "[{}]", s.join(", "))
            }
            Value::Map(m)           => write!(f, "{{map({} keys)}}", m.len()),
            Value::StructInstance(name, _) => write!(f, "{} {{...}}", name),
            Value::EnumVariant(e, v) => write!(f, "{}.{}", e, v),
            Value::Workflow(name)   => write!(f, "workflow:{}", name),
            Value::Future(v)        => write!(f, "future({})", v),
        }
    }
}

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct RuntimeError(pub String);

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RuntimeError: {}", self.0)
    }
}

type Result<T> = std::result::Result<T, RuntimeError>;

// Control-flow signals
enum Signal {
    Return(Option<Value>),
}

// ---------------------------------------------------------------------------
// Environment (scoped variable store)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Env {
    vars: HashMap<String, Value>,
    parent: Option<Box<Env>>,
}

impl Env {
    pub fn new() -> Self { Self::default() }

    pub fn child(parent: Env) -> Self {
        Self { vars: HashMap::new(), parent: Some(Box::new(parent)) }
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.vars.get(name).or_else(|| self.parent.as_ref()?.get(name))
    }

    pub fn set(&mut self, name: String, value: Value) {
        self.vars.insert(name, value);
    }
}

// ---------------------------------------------------------------------------
// Interpreter
// ---------------------------------------------------------------------------

pub struct Interpreter {
    /// Global workflow/enum/struct registry
    workflows: HashMap<String, WorkflowDecl>,
    enums: HashMap<String, EnumDecl>,
    structs: HashMap<String, StructDecl>,
    /// Output sink (can be overridden in tests)
    output: Vec<String>,
    /// Dry-run mode: print torii commands instead of executing
    pub dry_run: bool,
}

impl Interpreter {
    pub fn new() -> Self {
        Self {
            workflows: HashMap::new(),
            enums: HashMap::new(),
            structs: HashMap::new(),
            output: Vec::new(),
            dry_run: false,
        }
    }

    pub fn load(&mut self, program: Program) {
        for item in program.items {
            match item {
                Item::Workflow(w) => { self.workflows.insert(w.name.clone(), w); }
                Item::Enum(e)     => { self.enums.insert(e.name.clone(), e); }
                Item::Struct(s)   => { self.structs.insert(s.name.clone(), s); }
                Item::Impl(_)     => {} // stored with struct (future)
                Item::Import(_)   => {} // handled by loader (future)
            }
        }
    }

    /// Run a named workflow with positional arguments
    pub fn run(&mut self, name: &str, args: Vec<Value>) -> Result<Option<Value>> {
        let wf = self.workflows.get(name)
            .ok_or_else(|| RuntimeError(format!("workflow '{}' not found", name)))?
            .clone();

        let mut env = Env::new();

        // Bind params
        for (i, param) in wf.params.iter().enumerate() {
            let val = args.get(i)
                .cloned()
                .or_else(|| {
                    param.default.as_ref().and_then(|d| {
                        let mut tmp_env = Env::new();
                        self.eval_expr(d, &mut tmp_env).ok()
                    })
                })
                .unwrap_or(Value::Null);
            env.set(param.name.clone(), val);
        }

        match self.exec_block(&wf.body, &mut env) {
            Ok(()) => Ok(None),
            Err(e) if e.0.starts_with("__return__:") => {
                // Encoded return value
                Ok(None) // simplified: return values handled via Signal
            }
            Err(e) => {
                // Try on_error handler
                if let Some(handler) = &wf.body.on_error.clone() {
                    eprintln!("⚠️  Error in workflow '{}': {}", name, e);
                    let mut err_env = Env::child(env);
                    self.exec_block(handler, &mut err_env)?;
                }
                Err(e)
            }
        }
    }

    // -----------------------------------------------------------------------
    // Block execution
    // -----------------------------------------------------------------------

    fn exec_block(&mut self, block: &Block, env: &mut Env) -> Result<()> {
        for stmt in &block.stmts {
            self.exec_stmt(stmt, env)?;
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Statement execution
    // -----------------------------------------------------------------------

    fn exec_stmt(&mut self, stmt: &Stmt, env: &mut Env) -> Result<()> {
        match stmt {
            Stmt::Assign(a) => {
                let val = self.eval_expr(&a.value, env)?;
                env.set(a.target.clone(), val);
            }
            Stmt::Expr(e) => {
                self.eval_expr(e, env)?;
            }
            Stmt::Return(e) => {
                let _val = e.as_ref().map(|ex| self.eval_expr(ex, env)).transpose()?;
                return Err(RuntimeError("__return__".to_string()));
            }
            Stmt::If(i) => {
                let cond = self.eval_expr(&i.condition, env)?;
                if is_truthy(&cond) {
                    let mut child = Env::child(env.clone());
                    self.exec_block(&i.then_block, &mut child)?;
                } else if let Some(else_b) = &i.else_block {
                    let mut child = Env::child(env.clone());
                    self.exec_block(else_b, &mut child)?;
                }
            }
            Stmt::For(f) => {
                let iterable = self.eval_expr(&f.iterable, env)?;
                let items = coerce_to_list(iterable);
                for item in items {
                    let mut child = Env::child(env.clone());
                    child.set(f.var.clone(), item);
                    self.exec_block(&f.body, &mut child)?;
                }
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Expression evaluation
    // -----------------------------------------------------------------------

    pub fn eval_expr(&mut self, expr: &Expr, env: &mut Env) -> Result<Value> {
        match expr {
            Expr::StringLit(s)       => Ok(Value::String(interpolate(s, env))),
            Expr::NumberLit(n)       => Ok(Value::Number(*n)),
            Expr::BoolLit(b)         => Ok(Value::Bool(*b)),
            Expr::Null               => Ok(Value::Null),
            Expr::RegexLit(r)        => Ok(Value::String(format!("/{}/", r))),
            Expr::DurationLit(v, u)  => Ok(Value::Duration(*v, u.clone())),

            Expr::Ident(name) => {
                env.get(name).cloned()
                    .ok_or_else(|| RuntimeError(format!("undefined variable '{}'", name)))
            }

            Expr::List(items) => {
                let vals: Result<Vec<Value>> = items.iter()
                    .map(|e| self.eval_expr(e, env))
                    .collect();
                Ok(Value::List(vals?))
            }

            Expr::Map(pairs) => {
                let mut map = HashMap::new();
                for (k, v) in pairs {
                    map.insert(k.clone(), self.eval_expr(v, env)?);
                }
                Ok(Value::Map(map))
            }

            Expr::StructInit(name, fields) => {
                let mut map = HashMap::new();
                for (k, v) in fields {
                    map.insert(k.clone(), self.eval_expr(v, env)?);
                }
                Ok(Value::StructInstance(name.clone(), map))
            }

            Expr::Member(obj, field) => {
                let obj_val = self.eval_expr(obj, env)?;
                match &obj_val {
                    Value::StructInstance(_, fields) => fields.get(field).cloned()
                        .ok_or_else(|| RuntimeError(format!("field '{}' not found", field))),
                    Value::Map(m) => m.get(field).cloned()
                        .ok_or_else(|| RuntimeError(format!("key '{}' not found", field))),
                    _ => Err(RuntimeError(format!("cannot access field '{}' on {:?}", field, obj_val))),
                }
            }

            Expr::Call(call) => self.eval_call(call, env),

            Expr::Async(inner) => {
                // For now execute synchronously and wrap in Future
                let val = self.eval_expr(inner, env)?;
                Ok(Value::Future(Box::new(val)))
            }

            Expr::Await(inner, timeout) => {
                let val = self.eval_expr(inner, env)?;
                // timeout is informational for now
                if let Some(_t) = timeout {
                    // future: register timeout handler
                }
                match val {
                    Value::Future(inner) => Ok(*inner),
                    other => Ok(other),
                }
            }

            Expr::AwaitAll(futures_expr, _timeout) => {
                let futures = self.eval_expr(futures_expr, env)?;
                match futures {
                    Value::List(items) => {
                        // Resolve all futures sequentially
                        let results: Vec<Value> = items.into_iter().map(|f| match f {
                            Value::Future(v) => *v,
                            other => other,
                        }).collect();
                        Ok(Value::List(results))
                    }
                    other => Ok(other),
                }
            }

            Expr::BinOp(left, op, right) => {
                let l = self.eval_expr(left, env)?;
                let r = self.eval_expr(right, env)?;
                eval_binop(l, op, r)
            }

            Expr::UnaryOp(op, inner) => {
                let val = self.eval_expr(inner, env)?;
                match op {
                    UnaryOp::Not => Ok(Value::Bool(!is_truthy(&val))),
                    UnaryOp::Neg => match val {
                        Value::Number(n) => Ok(Value::Number(-n)),
                        _ => Err(RuntimeError("unary minus requires a number".to_string())),
                    },
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Function / method calls
    // -----------------------------------------------------------------------

    fn eval_call(&mut self, call: &CallExpr, env: &mut Env) -> Result<Value> {
        // Resolve callee
        match call.callee.as_ref() {
            Expr::Ident(name) => self.call_builtin_or_workflow(name, &call.args, env),
            Expr::Member(obj, method) => {
                let obj_val = self.eval_expr(obj, env)?;
                self.call_method(obj_val, method, &call.args, env)
            }
            other => {
                let val = self.eval_expr(other, env)?;
                Err(RuntimeError(format!("cannot call {:?}", val)))
            }
        }
    }

    fn eval_args(&mut self, args: &[Arg], env: &mut Env) -> Result<Vec<Value>> {
        args.iter().map(|a| match a {
            Arg::Positional(e) => self.eval_expr(e, env),
            Arg::Named(_, e)   => self.eval_expr(e, env),
        }).collect()
    }

    fn call_builtin_or_workflow(&mut self, name: &str, args: &[Arg], env: &mut Env) -> Result<Value> {
        let vals = self.eval_args(args, env)?;

        match name {
            // --- I/O ---
            "print" => {
                let msg = vals.first().map(|v| v.to_string()).unwrap_or_default();
                println!("{}", msg);
                self.output.push(msg);
                Ok(Value::Null)
            }
            "input" => {
                let prompt = vals.first().map(|v| v.to_string()).unwrap_or_default();
                print!("{}: ", prompt);
                use std::io::Write;
                std::io::stdout().flush().ok();
                let mut line = String::new();
                std::io::stdin().read_line(&mut line).ok();
                Ok(Value::String(line.trim().to_string()))
            }
            "confirm" => {
                let prompt = vals.first().map(|v| v.to_string()).unwrap_or_default();
                print!("{} [y/N] ", prompt);
                use std::io::Write;
                std::io::stdout().flush().ok();
                let mut line = String::new();
                std::io::stdin().read_line(&mut line).ok();
                let answer = line.trim().to_lowercase();
                if answer != "y" && answer != "yes" {
                    return Err(RuntimeError("Cancelled by user".to_string()));
                }
                Ok(Value::Bool(true))
            }
            "notify" => {
                let msg = vals.first().map(|v| v.to_string()).unwrap_or_default();
                self.run_torii(&["notify", &msg])?;
                Ok(Value::Null)
            }

            // --- Core torii commands ---
            "save" => {
                let msg = vals.first().map(|v| v.to_string()).unwrap_or_default();
                self.run_torii(&["save", "-am", &msg])?;
                Ok(Value::Null)
            }
            "sync" => {
                let force = named_bool(args, "push").unwrap_or(false);
                if force {
                    self.run_torii(&["sync", "--push"])?;
                } else {
                    self.run_torii(&["sync"])?;
                }
                Ok(Value::Null)
            }

            // --- User-defined workflow ---
            _ if self.workflows.contains_key(name) => {
                let result = self.run(name, vals)?;
                Ok(result.unwrap_or(Value::Null))
            }

            _ => Err(RuntimeError(format!("unknown function '{}'", name))),
        }
    }

    fn call_method(&mut self, obj: Value, method: &str, args: &[Arg], env: &mut Env) -> Result<Value> {
        let vals = self.eval_args(args, env)?;

        match (&obj, method) {
            // snapshot.create / restore / list
            (Value::String(ns), "create") if ns == "snapshot" => {
                let name = vals.first().map(|v| v.to_string()).unwrap_or_default();
                self.run_torii(&["snapshot", "create", "-n", &name])?;
                Ok(Value::Null)
            }
            (Value::String(ns), "restore") if ns == "snapshot" => {
                let id = vals.first().map(|v| v.to_string()).unwrap_or_default();
                self.run_torii(&["snapshot", "restore", &id])?;
                Ok(Value::Null)
            }
            // mirror.sync
            (Value::String(ns), "sync") if ns == "mirror" => {
                self.run_torii(&["mirror", "sync"])?;
                Ok(Value::Null)
            }
            // tag.release / tag.create
            (Value::String(ns), "release") if ns == "tag" => {
                if let Some(v) = vals.first() {
                    self.run_torii(&["tag", "release", "--bump", &v.to_string()])?;
                } else {
                    self.run_torii(&["tag", "release"])?;
                }
                Ok(Value::Null)
            }
            // semver.bump
            (Value::String(ns), "bump") if ns == "semver" => {
                let kind = vals.first().map(|v| v.to_string()).unwrap_or("minor".to_string());
                self.run_torii(&["tag", "release", "--bump", &kind, "--dry-run"])?;
                Ok(Value::String(format!("bumped-{}", kind)))
            }
            // notify.to / notify.channel
            (Value::String(ns), "to") if ns == "notify" => {
                let channel = vals.first().map(|v| v.to_string()).unwrap_or_default();
                let msg = vals.get(1).map(|v| v.to_string()).unwrap_or_default();
                println!("[notify → {}] {}", channel, msg);
                Ok(Value::Null)
            }
            (Value::String(ns), "channel") if ns == "notify" => {
                println!("[notify.channel configured]");
                Ok(Value::Null)
            }
            // string methods
            (Value::String(s), "matches") => {
                let pattern = vals.first().map(|v| v.to_string()).unwrap_or_default();
                Ok(Value::Bool(s.contains(&pattern)))
            }
            (Value::String(s), "len") => Ok(Value::Number(s.len() as f64)),
            // list methods
            (Value::List(items), "push") => {
                let mut new_items = items.clone();
                if let Some(v) = vals.first() {
                    new_items.push(v.clone());
                }
                Ok(Value::List(new_items))
            }
            (Value::List(items), "len") => Ok(Value::Number(items.len() as f64)),
            _ => Err(RuntimeError(format!("unknown method '{}'", method))),
        }
    }

    // -----------------------------------------------------------------------
    // Torii command runner
    // -----------------------------------------------------------------------

    fn run_torii(&mut self, args: &[&str]) -> Result<()> {
        if self.dry_run {
            println!("[dry-run] torii {}", args.join(" "));
            return Ok(());
        }

        let status = std::process::Command::new("torii")
            .args(args)
            .status()
            .map_err(|e| RuntimeError(format!("failed to run torii: {}", e)))?;

        if !status.success() {
            return Err(RuntimeError(format!(
                "torii {} failed with exit code {:?}",
                args.join(" "),
                status.code()
            )));
        }
        Ok(())
    }

    /// Captured output (useful for testing)
    pub fn output(&self) -> &[String] {
        &self.output
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn is_truthy(val: &Value) -> bool {
    match val {
        Value::Bool(b)   => *b,
        Value::Null      => false,
        Value::Number(n) => *n != 0.0,
        Value::String(s) => !s.is_empty(),
        Value::List(l)   => !l.is_empty(),
        _                => true,
    }
}

fn coerce_to_list(val: Value) -> Vec<Value> {
    match val {
        Value::List(items) => items,
        other => vec![other],
    }
}

fn eval_binop(l: Value, op: &BinOp, r: Value) -> Result<Value> {
    match op {
        BinOp::Add => match (l, r) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a + b)),
            (Value::String(a), Value::String(b)) => Ok(Value::String(a + &b)),
            _ => Err(RuntimeError("+ requires two numbers or two strings".to_string())),
        },
        BinOp::Sub => num_op(l, r, |a, b| a - b),
        BinOp::Mul => num_op(l, r, |a, b| a * b),
        BinOp::Div => num_op(l, r, |a, b| a / b),
        BinOp::Eq    => Ok(Value::Bool(values_eq(&l, &r))),
        BinOp::NotEq => Ok(Value::Bool(!values_eq(&l, &r))),
        BinOp::Lt  => cmp_op(l, r, |a, b| a < b),
        BinOp::Gt  => cmp_op(l, r, |a, b| a > b),
        BinOp::LtEq => cmp_op(l, r, |a, b| a <= b),
        BinOp::GtEq => cmp_op(l, r, |a, b| a >= b),
        BinOp::And => Ok(Value::Bool(is_truthy(&l) && is_truthy(&r))),
        BinOp::Or  => Ok(Value::Bool(is_truthy(&l) || is_truthy(&r))),
    }
}

fn num_op(l: Value, r: Value, f: fn(f64, f64) -> f64) -> Result<Value> {
    match (l, r) {
        (Value::Number(a), Value::Number(b)) => Ok(Value::Number(f(a, b))),
        _ => Err(RuntimeError("arithmetic requires numbers".to_string())),
    }
}

fn cmp_op(l: Value, r: Value, f: fn(f64, f64) -> bool) -> Result<Value> {
    match (l, r) {
        (Value::Number(a), Value::Number(b)) => Ok(Value::Bool(f(a, b))),
        _ => Err(RuntimeError("comparison requires numbers".to_string())),
    }
}

fn values_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::String(x), Value::String(y))   => x == y,
        (Value::Number(x), Value::Number(y))   => x == y,
        (Value::Bool(x), Value::Bool(y))       => x == y,
        (Value::Null, Value::Null)             => true,
        _ => false,
    }
}

/// Resolve {var} placeholders in string literals
fn interpolate(template: &str, env: &Env) -> String {
    let mut result = String::new();
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '{' {
            let mut name = String::new();
            for inner in chars.by_ref() {
                if inner == '}' { break; }
                name.push(inner);
            }
            let val = env.get(name.trim())
                .map(|v| v.to_string())
                .unwrap_or_else(|| format!("{{{}}}", name));
            result.push_str(&val);
        } else {
            result.push(ch);
        }
    }

    result
}

/// Extract a named boolean argument from arg list
fn named_bool(args: &[Arg], key: &str) -> Option<bool> {
    for arg in args {
        if let Arg::Named(k, Expr::BoolLit(b)) = arg {
            if k == key { return Some(*b); }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate::lexer::Lexer;
    use crate::gate::parser::Parser;

    fn run(src: &str, workflow: &str) -> Interpreter {
        let mut lexer = Lexer::new(src);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse().unwrap();
        let mut interp = Interpreter::new();
        interp.dry_run = true;
        interp.load(program);
        interp.run(workflow, vec![]).unwrap();
        interp
    }

    #[test]
    fn test_print() {
        let interp = run(r#"
workflow hello() {
    print("Hello, Gate!")
}
"#, "hello");
        assert_eq!(interp.output(), &["Hello, Gate!"]);
    }

    #[test]
    fn test_variable_interpolation() {
        let interp = run(r#"
workflow greet() {
    name = "world"
    print("Hello, {name}!")
}
"#, "greet");
        assert_eq!(interp.output(), &["Hello, world!"]);
    }

    #[test]
    fn test_if_true() {
        let interp = run(r#"
workflow check() {
    x = 10
    if x > 5 {
        print("big")
    } else {
        print("small")
    }
}
"#, "check");
        assert_eq!(interp.output(), &["big"]);
    }

    #[test]
    fn test_for_loop() {
        let interp = run(r#"
workflow items() {
    for item in ["a", "b", "c"] {
        print(item)
    }
}
"#, "items");
        assert_eq!(interp.output(), &["a", "b", "c"]);
    }

    #[test]
    fn test_workflow_call() {
        let interp = run(r#"
workflow greet(name) {
    print("Hi {name}")
}

workflow main() {
    greet("Gate")
}
"#, "main");
        assert_eq!(interp.output(), &["Hi Gate"]);
    }

    #[test]
    fn test_binop() {
        let interp = run(r#"
workflow math() {
    x = 3 + 4
    print("{x}")
}
"#, "math");
        assert_eq!(interp.output(), &["7"]);
    }

    #[test]
    fn test_dry_run_save() {
        let interp = run(r#"
workflow deploy() {
    save("feat: deploy")
    sync(push: true)
}
"#, "deploy");
        // dry_run = true so torii is not actually called, no error
        let _ = interp;
    }
}
