use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use crate::compiler::Compiler;
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::vm::VM;

const PROMPT: &str = "kria> ";
const CONT_PROMPT: &str = "kria...> ";

pub struct ReplSession {
    compiler: Compiler,
    vm: VM,
    next_ip: usize,
}

impl ReplSession {
    pub fn new() -> Self {
        ReplSession {
            compiler: Compiler::new(),
            vm: VM::new(),
            next_ip: 0,
        }
    }

    fn reset(&mut self) {
        self.compiler = Compiler::new();
        self.vm = VM::new();
        self.next_ip = 0;
        println!("Session reset.");
    }

    pub fn eval(&mut self, source: &str) -> Result<(), String> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);
        let statements = parser.parse()?;

        for stmt in &statements {
            if matches!(stmt, crate::ast::Statement::Import { .. }) {
                return Err("imports are not supported in the REPL yet".to_string());
            }
        }

        self.compiler.compile_repl(&statements)?;
        let bytecode = self.compiler.bytecode();
        self.next_ip = self.vm.execute_from(bytecode, self.next_ip)?;
        Ok(())
    }

    fn handle_meta(&mut self, line: &str) -> MetaResult {
        match line.trim() {
            ":exit" | ":quit" => MetaResult::Exit,
            ":help" | ":h" => {
                print_help();
                MetaResult::Handled
            }
            ":reset" => {
                self.reset();
                MetaResult::Handled
            }
            _ if line.starts_with(':') => {
                eprintln!("Unknown command: {}. Type :help for help.", line.trim());
                MetaResult::Handled
            }
            _ => MetaResult::NotMeta,
        }
    }
}

enum MetaResult {
    Exit,
    Handled,
    NotMeta,
}

fn print_help() {
    println!(
        r#"Kria REPL
---------
Enter Kria statements or expressions. State persists between lines.

  set x = 10       define a variable
  x + 5            expression result is printed automatically
  print(x)         explicit print
  fn f(n) {{         multi-line: open {{ ( [ until closed
      return n * 2
  }}

Commands:
  :help   show this help
  :reset  clear variables and functions
  :exit   leave the REPL (also :quit)
"#
    );
}

/// Delimiter depth outside of strings and comments.
fn open_delimiter_depth(source: &str) -> i32 {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut in_triple_string = false;
    let mut escape = false;
    let mut chars = source.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_triple_string {
            if ch == '"' && chars.peek() == Some(&'"') {
                chars.next();
                if chars.peek() == Some(&'"') {
                    chars.next();
                    in_triple_string = false;
                }
            }
            continue;
        }

        if in_string {
            if escape {
                escape = false;
                continue;
            }
            if ch == '\\' {
                escape = true;
                continue;
            }
            if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '/' && chars.peek() == Some(&'/') {
            chars.next();
            while let Some(c) = chars.next() {
                if c == '\n' {
                    break;
                }
            }
            continue;
        }

        if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            while let Some(c) = chars.next() {
                if c == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
            }
            continue;
        }

        match ch {
            '"' => {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        in_triple_string = true;
                    }
                } else {
                    in_string = true;
                }
            }
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => depth -= 1,
            _ => {}
        }
    }
    depth
}

fn read_block(editor: &mut DefaultEditor) -> Result<Option<String>, ReadlineError> {
    let mut buffer = String::new();

    loop {
        let prompt = if buffer.is_empty() {
            PROMPT
        } else {
            CONT_PROMPT
        };
        let line = editor.readline(prompt)?;
        let trimmed = line.trim();
        if buffer.is_empty() && trimmed.is_empty() {
            return Ok(None);
        }
        if !buffer.is_empty() {
            buffer.push('\n');
        }
        buffer.push_str(&line);
        if open_delimiter_depth(&buffer) <= 0 {
            break;
        }
    }
    Ok(Some(buffer))
}

pub fn run() -> Result<(), String> {
    println!("Kria REPL — type :help for help, :exit to quit.");
    let mut editor =
        DefaultEditor::new().map_err(|e| format!("Failed to initialize line editor: {}", e))?;
    let mut session = ReplSession::new();

    loop {
        let input = match read_block(&mut editor) {
            Ok(Some(s)) => s,
            Ok(None) => continue,
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => {
                println!();
                break;
            }
            Err(e) => return Err(format!("Read error: {}", e)),
        };

        let _ = editor.add_history_entry(input.trim());

        match session.handle_meta(&input) {
            MetaResult::Exit => break,
            MetaResult::Handled => continue,
            MetaResult::NotMeta => {}
        }

        match session.eval(&input) {
            Ok(()) => {}
            Err(e) => eprintln!("{}", e),
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delimiter_depth_ignores_strings() {
        assert_eq!(open_delimiter_depth(r#"{ "}" "#), 1);
        assert_eq!(open_delimiter_depth("print(\"\"\"a\nb\"\"\")"), 0);
        assert_eq!(open_delimiter_depth("/* { */ set x = 1"), 0);
        assert_eq!(open_delimiter_depth("fn f() {\n  return 1\n}"), 0);
    }

    #[test]
    fn persistent_globals() {
        let mut session = ReplSession::new();
        session.eval("set x = 10").unwrap();
        session.eval("x + 5").unwrap();
        session.reset();
        session.eval("set y = 1").unwrap();
    }
}
