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

/// Internal control-flow signal. `Return` carries an optional value so callers
/// can capture `return expr` results without parsing magic strings.
#[derive(Debug, Clone)]
pub enum Signal {
    Return(Option<Value>),
}

/// Errors that may flow through interpreter — either a real runtime error or
/// a control-flow signal (return, break, continue in the future).
#[derive(Debug, Clone)]
pub enum Flow {
    Error(RuntimeError),
    Signal(Signal),
}

impl From<RuntimeError> for Flow {
    fn from(e: RuntimeError) -> Self { Flow::Error(e) }
}

type FlowResult<T> = std::result::Result<T, Flow>;

// ---------------------------------------------------------------------------
// Environment (scoped variable store)
// ---------------------------------------------------------------------------

/// Lexically scoped environment. Implemented as a stack of frames so an inner
/// scope (if/for/block) can read+write variables that live in any outer frame
/// without cloning the parent — assignments to existing names update the frame
/// where they were originally defined.
#[derive(Debug, Clone, Default)]
pub struct Env {
    frames: Vec<HashMap<String, Value>>,
}

impl Env {
    pub fn new() -> Self {
        Self { frames: vec![HashMap::new()] }
    }

    pub fn push_scope(&mut self) {
        self.frames.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        if self.frames.len() > 1 {
            self.frames.pop();
        }
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        for frame in self.frames.iter().rev() {
            if let Some(v) = frame.get(name) { return Some(v); }
        }
        None
    }

    /// Set a variable. If `name` already exists in any outer frame, update it
    /// in place. Otherwise create it in the innermost frame.
    pub fn set(&mut self, name: String, value: Value) {
        for frame in self.frames.iter_mut().rev() {
            if frame.contains_key(&name) {
                frame.insert(name, value);
                return;
            }
        }
        if let Some(top) = self.frames.last_mut() {
            top.insert(name, value);
        }
    }

    /// Force-set a variable in the innermost frame (for loop bindings).
    pub fn set_local(&mut self, name: String, value: Value) {
        if let Some(top) = self.frames.last_mut() {
            top.insert(name, value);
        }
    }

    /// Backwards-compat shim used by older code paths.
    #[deprecated(note = "use push_scope/pop_scope instead")]
    pub fn child(parent: Env) -> Self { parent }
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
                Item::Policy(_)   => {} // policies are consumed by external tools (torii commit-scan, CI gates)
            }
        }
    }

    /// Run a named workflow with positional arguments. Returns the value of a
    /// top-level `return expr` if the workflow used one, otherwise None.
    pub fn run(&mut self, name: &str, args: Vec<Value>) -> Result<Option<Value>> {
        self.run_with_named(name, args, &[])
    }

    /// Run a named workflow with positional + named arguments. Named args take
    /// precedence over positionals when both reference the same parameter.
    pub fn run_with_named(
        &mut self,
        name: &str,
        positional: Vec<Value>,
        named: &[(String, Value)],
    ) -> Result<Option<Value>> {
        let wf = self.workflows.get(name)
            .ok_or_else(|| RuntimeError(format!("workflow '{}' not found", name)))?
            .clone();

        let mut env = Env::new();

        for (i, param) in wf.params.iter().enumerate() {
            let val = named.iter().find(|(k, _)| k == &param.name).map(|(_, v)| v.clone())
                .or_else(|| positional.get(i).cloned())
                .or_else(|| {
                    param.default.as_ref().and_then(|d| {
                        let mut tmp_env = Env::new();
                        self.eval_expr(d, &mut tmp_env).ok()
                    })
                })
                .unwrap_or(Value::Null);
            env.set_local(param.name.clone(), val);
        }

        match self.exec_block(&wf.body, &mut env) {
            Ok(()) => Ok(None),
            Err(Flow::Signal(Signal::Return(val))) => Ok(val),
            Err(Flow::Error(e)) => {
                if let Some(handler) = &wf.body.on_error.clone() {
                    eprintln!("⚠️  Error in workflow '{}': {}", name, e);
                    env.push_scope();
                    let handler_result = self.exec_block(handler, &mut env);
                    env.pop_scope();
                    match handler_result {
                        Ok(()) => {}
                        Err(Flow::Signal(Signal::Return(_))) => {}
                        Err(Flow::Error(handler_err)) => return Err(handler_err),
                    }
                }
                Err(e)
            }
        }
    }

    // -----------------------------------------------------------------------
    // Block execution
    // -----------------------------------------------------------------------

    fn exec_block(&mut self, block: &Block, env: &mut Env) -> FlowResult<()> {
        for stmt in &block.stmts {
            self.exec_stmt(stmt, env)?;
        }
        Ok(())
    }

    fn exec_stmt(&mut self, stmt: &Stmt, env: &mut Env) -> FlowResult<()> {
        match stmt {
            Stmt::Assign(a) => {
                let val = self.eval_expr(&a.value, env)?;
                env.set(a.target.clone(), val);
            }
            Stmt::Expr(e) => {
                self.eval_expr(e, env)?;
            }
            Stmt::Return(e) => {
                let val = e.as_ref().map(|ex| self.eval_expr(ex, env)).transpose()?;
                return Err(Flow::Signal(Signal::Return(val)));
            }
            Stmt::If(i) => {
                // No new scope — variables assigned inside if/else persist
                // (matches Python/JS-let semantics for control-flow blocks).
                let cond = self.eval_expr(&i.condition, env)?;
                if is_truthy(&cond) {
                    self.exec_block(&i.then_block, env)?;
                } else if let Some(else_b) = &i.else_block {
                    self.exec_block(else_b, env)?;
                }
            }
            Stmt::For(f) => {
                // For-loop variable lives in the workflow scope so it's still
                // readable after the loop completes (last value bound).
                let iterable = self.eval_expr(&f.iterable, env)?;
                let items = coerce_to_list(iterable);
                for item in items {
                    env.set(f.var.clone(), item);
                    self.exec_block(&f.body, env)?;
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
                // Builtin namespaces are resolved as marker strings so method
                // dispatch (`tag.release`, `mirror.sync`, …) works without the
                // user defining them. Real variables shadow these.
                if let Some(v) = env.get(name) {
                    return Ok(v.clone());
                }
                if is_builtin_namespace(name) {
                    return Ok(Value::String(name.to_string()));
                }
                // Treat enum/struct names as marker strings for downstream calls
                if self.enums.contains_key(name) || self.structs.contains_key(name) {
                    return Ok(Value::String(name.to_string()));
                }
                Err(RuntimeError(format!("undefined variable '{}'", name)))
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
        match call.callee.as_ref() {
            Expr::Ident(name) => self.call_builtin_or_workflow(name, &call.args, env),
            Expr::Member(obj, method) => {
                // Mutating list methods need to update the variable in place
                // when the receiver is a plain identifier.
                if matches!(method.as_str(), "push") {
                    if let Expr::Ident(var_name) = obj.as_ref() {
                        let positional = self.eval_positional(&call.args, env)?;
                        let current = env.get(var_name).cloned()
                            .ok_or_else(|| RuntimeError(format!("undefined variable '{}'", var_name)))?;
                        if let Value::List(mut items) = current {
                            if let Some(v) = positional.into_iter().next() {
                                items.push(v);
                            }
                            let updated = Value::List(items);
                            env.set(var_name.clone(), updated.clone());
                            return Ok(updated);
                        }
                    }
                }

                let obj_val = self.eval_expr(obj, env)?;
                self.call_method(obj_val, method, &call.args, env)
            }
            other => {
                let val = self.eval_expr(other, env)?;
                Err(RuntimeError(format!("cannot call {:?}", val)))
            }
        }
    }

    fn eval_positional(&mut self, args: &[Arg], env: &mut Env) -> Result<Vec<Value>> {
        args.iter().filter_map(|a| match a {
            Arg::Positional(e) => Some(self.eval_expr(e, env)),
            Arg::Named(_, _)   => None,
        }).collect()
    }

    fn eval_named(&mut self, args: &[Arg], env: &mut Env) -> Result<Vec<(String, Value)>> {
        args.iter().filter_map(|a| match a {
            Arg::Named(k, e) => Some(self.eval_expr(e, env).map(|v| (k.clone(), v))),
            Arg::Positional(_) => None,
        }).collect()
    }

    fn call_builtin_or_workflow(&mut self, name: &str, args: &[Arg], env: &mut Env) -> Result<Value> {
        let positional = self.eval_positional(args, env)?;
        let named = self.eval_named(args, env)?;

        match name {
            // --- I/O ---
            "print" => {
                let msg = positional.first().map(|v| v.to_string()).unwrap_or_default();
                println!("{}", msg);
                self.output.push(msg);
                Ok(Value::Null)
            }
            "input" => {
                let prompt = positional.first().map(|v| v.to_string()).unwrap_or_default();
                print!("{}: ", prompt);
                use std::io::Write;
                std::io::stdout().flush().ok();
                let mut line = String::new();
                std::io::stdin().read_line(&mut line).ok();
                Ok(Value::String(line.trim().to_string()))
            }
            "confirm" => {
                let prompt = positional.first().map(|v| v.to_string()).unwrap_or_default();
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
                // torii has no `notify` subcommand — surface the message instead.
                let msg = positional.first().map(|v| v.to_string()).unwrap_or_default();
                println!("[notify] {}", msg);
                self.output.push(format!("[notify] {}", msg));
                Ok(Value::Null)
            }

            // --- Core torii commands ---
            "save" => {
                let msg = positional.first().map(|v| v.to_string()).unwrap_or_default();
                let all   = lookup_named_bool(&named, "all").unwrap_or(false);
                let amend = lookup_named_bool(&named, "amend").unwrap_or(false);
                let mut argv: Vec<String> = vec!["save".to_string()];
                if all   { argv.push("-a".to_string()); }
                if amend { argv.push("--amend".to_string()); }
                argv.push("-m".to_string());
                argv.push(msg);
                let argv_ref: Vec<&str> = argv.iter().map(|s| s.as_str()).collect();
                self.run_torii(&argv_ref)?;
                Ok(Value::Null)
            }
            "sync" => {
                let push  = lookup_named_bool(&named, "push").unwrap_or(false);
                let pull  = lookup_named_bool(&named, "pull").unwrap_or(false);
                let force = lookup_named_bool(&named, "force").unwrap_or(false);
                let fetch = lookup_named_bool(&named, "fetch").unwrap_or(false);
                let mut argv: Vec<String> = vec!["sync".to_string()];
                if force { argv.push("--force".to_string()); }
                else if fetch { argv.push("--fetch".to_string()); }
                else if push  { argv.push("--push".to_string()); }
                else if pull  { argv.push("--pull".to_string()); }
                let argv_ref: Vec<&str> = argv.iter().map(|s| s.as_str()).collect();
                self.run_torii(&argv_ref)?;
                Ok(Value::Null)
            }

            // --- User-defined workflow ---
            _ if self.workflows.contains_key(name) => {
                let result = self.run_with_named(name, positional, &named)?;
                Ok(result.unwrap_or(Value::Null))
            }

            _ => Err(RuntimeError(format!("unknown function '{}'", name))),
        }
    }

    fn call_method(&mut self, obj: Value, method: &str, args: &[Arg], env: &mut Env) -> Result<Value> {
        let vals = self.eval_positional(args, env)?;

        match (&obj, method) {
            // snapshot.create / restore / list / delete
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
            (Value::String(ns), "list") if ns == "snapshot" => {
                self.run_torii(&["snapshot", "list"])?;
                Ok(Value::Null)
            }
            (Value::String(ns), "delete") if ns == "snapshot" => {
                let id = vals.first().map(|v| v.to_string()).unwrap_or_default();
                self.run_torii(&["snapshot", "delete", &id])?;
                Ok(Value::Null)
            }

            // mirror.sync / mirror.list
            (Value::String(ns), "sync") if ns == "mirror" => {
                self.run_torii(&["mirror", "sync"])?;
                Ok(Value::Null)
            }
            (Value::String(ns), "list") if ns == "mirror" => {
                self.run_torii(&["mirror", "list"])?;
                Ok(Value::Null)
            }

            // tag.create / tag.release / tag.list / tag.push / tag.delete
            // Note: torii v0.3.5+ moved `tag release` under `tag create --release`.
            (Value::String(ns), "create") if ns == "tag" => {
                let name = vals.first().map(|v| v.to_string()).unwrap_or_default();
                self.run_torii(&["tag", "create", &name])?;
                Ok(Value::Null)
            }
            (Value::String(ns), "release") if ns == "tag" => {
                if let Some(v) = vals.first() {
                    let bump = v.to_string();
                    self.run_torii(&["tag", "create", "--release", "--bump", &bump])?;
                } else {
                    self.run_torii(&["tag", "create", "--release"])?;
                }
                Ok(Value::Null)
            }
            (Value::String(ns), "list") if ns == "tag" => {
                self.run_torii(&["tag", "list"])?;
                Ok(Value::Null)
            }
            (Value::String(ns), "push") if ns == "tag" => {
                let name = vals.first().map(|v| v.to_string());
                if let Some(n) = name {
                    self.run_torii(&["tag", "push", &n])?;
                } else {
                    self.run_torii(&["tag", "push"])?;
                }
                Ok(Value::Null)
            }
            (Value::String(ns), "delete") if ns == "tag" => {
                let name = vals.first().map(|v| v.to_string()).unwrap_or_default();
                self.run_torii(&["tag", "delete", &name])?;
                Ok(Value::Null)
            }

            // semver.bump — dry-run preview, returns the next version string
            (Value::String(ns), "bump") if ns == "semver" => {
                let kind = vals.first().map(|v| v.to_string()).unwrap_or("minor".to_string());
                self.run_torii(&["tag", "create", "--release", "--bump", &kind, "--dry-run"])?;
                Ok(Value::String(format!("bumped-{}", kind)))
            }

            // branch.create / switch / delete / list / rename
            (Value::String(ns), "create") if ns == "branch" => {
                let name = vals.first().map(|v| v.to_string()).unwrap_or_default();
                self.run_torii(&["branch", &name, "-c"])?;
                Ok(Value::Null)
            }
            (Value::String(ns), "switch") if ns == "branch" => {
                let name = vals.first().map(|v| v.to_string()).unwrap_or_default();
                self.run_torii(&["branch", &name])?;
                Ok(Value::Null)
            }
            (Value::String(ns), "delete") if ns == "branch" => {
                let name = vals.first().map(|v| v.to_string()).unwrap_or_default();
                self.run_torii(&["branch", "-d", &name])?;
                Ok(Value::Null)
            }
            (Value::String(ns), "list") if ns == "branch" => {
                self.run_torii(&["branch", "--list"])?;
                Ok(Value::Null)
            }
            (Value::String(ns), "rename") if ns == "branch" => {
                let new_name = vals.first().map(|v| v.to_string()).unwrap_or_default();
                self.run_torii(&["branch", "--rename", &new_name])?;
                Ok(Value::Null)
            }

            // scan.staged / scan.history
            (Value::String(ns), "staged") if ns == "scan" => {
                self.run_torii(&["scan"])?;
                Ok(Value::Null)
            }
            (Value::String(ns), "history") if ns == "scan" => {
                self.run_torii(&["scan", "--history"])?;
                Ok(Value::Null)
            }

            // notify.to / notify.channel
            (Value::String(ns), "to") if ns == "notify" => {
                let channel = vals.first().map(|v| v.to_string()).unwrap_or_default();
                let msg = vals.get(1).map(|v| v.to_string()).unwrap_or_default();
                println!("[notify → {}] {}", channel, msg);
                self.output.push(format!("[notify → {}] {}", channel, msg));
                Ok(Value::Null)
            }
            (Value::String(ns), "channel") if ns == "notify" => {
                let name = vals.first().map(|v| v.to_string()).unwrap_or_default();
                let target = vals.get(1).map(|v| v.to_string()).unwrap_or_default();
                println!("[notify.channel configured: {} → {}]", name, target);
                Ok(Value::Null)
            }

            // string methods
            (Value::String(s), "matches") => {
                let pattern = vals.first().map(|v| v.to_string()).unwrap_or_default();
                Ok(Value::Bool(s.contains(&pattern)))
            }
            (Value::String(s), "len") => Ok(Value::Number(s.len() as f64)),
            (Value::String(s), "upper") => Ok(Value::String(s.to_uppercase())),
            (Value::String(s), "lower") => Ok(Value::String(s.to_lowercase())),
            (Value::String(s), "trim")  => Ok(Value::String(s.trim().to_string())),

            // list methods (non-mutating fallback — `push` mutating path is in eval_call)
            (Value::List(items), "push") => {
                let mut new_items = items.clone();
                if let Some(v) = vals.first() { new_items.push(v.clone()); }
                Ok(Value::List(new_items))
            }
            (Value::List(items), "len")   => Ok(Value::Number(items.len() as f64)),
            (Value::List(items), "first") => Ok(items.first().cloned().unwrap_or(Value::Null)),
            (Value::List(items), "last")  => Ok(items.last().cloned().unwrap_or(Value::Null)),

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

/// Resolve `{expr}` placeholders in string literals. Supports plain identifiers
/// (`{name}`) and dotted member access (`{config.env}`, `{user.profile.name}`).
/// Anything more complex (calls, arithmetic) is left as literal text — for now.
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
            let trimmed = name.trim();
            let val = resolve_path(trimmed, env)
                .map(|v| v.to_string())
                .unwrap_or_else(|| format!("{{{}}}", trimmed));
            result.push_str(&val);
        } else {
            result.push(ch);
        }
    }

    result
}

/// Walk a dotted path (`a.b.c`) through nested struct/map values.
fn resolve_path(path: &str, env: &Env) -> Option<Value> {
    let mut parts = path.split('.');
    let head = parts.next()?;
    let mut cur = env.get(head.trim())?.clone();
    for seg in parts {
        let seg = seg.trim();
        cur = match cur {
            Value::StructInstance(_, fields) => fields.get(seg).cloned()?,
            Value::Map(m)                    => m.get(seg).cloned()?,
            _ => return None,
        };
    }
    Some(cur)
}

/// Names that resolve as builtin namespace markers when used as bare idents.
/// Method dispatch (e.g. `tag.release(...)`) handles the actual behavior.
fn is_builtin_namespace(name: &str) -> bool {
    matches!(
        name,
        "tag" | "mirror" | "snapshot" | "branch" | "scan"
            | "history" | "remote" | "notify" | "semver" | "repo"
    )
}

/// Find the boolean value of a named argument that has already been evaluated.
fn lookup_named_bool(named: &[(String, Value)], key: &str) -> Option<bool> {
    named.iter().find(|(k, _)| k == key).and_then(|(_, v)| match v {
        Value::Bool(b) => Some(*b),
        _ => None,
    })
}

/// Find a named argument's evaluated value (any type).
#[allow(dead_code)]
fn lookup_named<'a>(named: &'a [(String, Value)], key: &str) -> Option<&'a Value> {
    named.iter().find(|(k, _)| k == key).map(|(_, v)| v)
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

    // -----------------------------------------------------------------
    // Regression tests for fixes (B1/B2/B6/B7/B9/B10/B4)
    // -----------------------------------------------------------------

    fn run_capture(src: &str, workflow: &str) -> Result<Option<Value>> {
        let mut lexer = Lexer::new(src);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse().unwrap();
        let mut interp = Interpreter::new();
        interp.dry_run = true;
        interp.load(program);
        interp.run(workflow, vec![])
    }

    /// B1+B2: `return expr` actually carries the value back to the caller.
    #[test]
    fn test_return_value() {
        let result = run_capture(r#"
workflow get() {
    return 42
}
"#, "get").unwrap();
        match result {
            Some(Value::Number(n)) => assert_eq!(n, 42.0),
            other => panic!("expected Number(42), got {:?}", other),
        }
    }

    /// B1+B2: workflow A calls workflow B that returns a value — A captures it.
    #[test]
    fn test_return_value_propagates_to_caller() {
        let mut lexer = Lexer::new(r#"
workflow inner() {
    return "hello"
}

workflow outer() {
    msg = inner()
    print(msg)
}
"#);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse().unwrap();
        let mut interp = Interpreter::new();
        interp.dry_run = true;
        interp.load(program);
        interp.run("outer", vec![]).unwrap();
        assert_eq!(interp.output(), &["hello"]);
    }

    /// B9: variables assigned inside an `if` block remain visible afterwards.
    #[test]
    fn test_if_scope_persists() {
        let interp = run(r#"
workflow check() {
    if true {
        result = "found"
    }
    print(result)
}
"#, "check");
        assert_eq!(interp.output(), &["found"]);
    }

    /// B9: variables assigned inside a `for` body remain visible afterwards.
    #[test]
    fn test_for_scope_persists() {
        let interp = run(r#"
workflow loop() {
    last = "none"
    for x in ["a", "b", "c"] {
        last = x
    }
    print(last)
}
"#, "loop");
        assert_eq!(interp.output(), &["c"]);
    }

    /// B10: `list.push` mutates the binding when called on an identifier.
    #[test]
    fn test_list_push_mutates() {
        let interp = run(r#"
workflow build() {
    items = []
    items.push("a")
    items.push("b")
    items.push("c")
    for x in items {
        print(x)
    }
}
"#, "build");
        assert_eq!(interp.output(), &["a", "b", "c"]);
    }

    /// B4: workflow accepts named arguments and routes them to params.
    #[test]
    fn test_workflow_named_args() {
        let interp = run(r#"
workflow greet(first, last) {
    print("{first} {last}")
}

workflow main() {
    greet(last: "Doe", first: "Jane")
}
"#, "main");
        assert_eq!(interp.output(), &["Jane Doe"]);
    }

    /// Workflow defaults still apply when neither positional nor named is passed.
    #[test]
    fn test_workflow_param_default() {
        let interp = run(r#"
workflow greet(name = "world") {
    print("Hello, {name}!")
}

workflow main() {
    greet()
}
"#, "main");
        assert_eq!(interp.output(), &["Hello, world!"]);
    }

    /// Calling `return` inside a nested if still returns from the workflow.
    #[test]
    fn test_return_from_nested_block() {
        let result = run_capture(r#"
workflow get() {
    x = 5
    if x > 0 {
        return "positive"
    }
    return "non-positive"
}
"#, "get").unwrap();
        match result {
            Some(Value::String(s)) => assert_eq!(s, "positive"),
            other => panic!("expected String, got {:?}", other),
        }
    }
}
