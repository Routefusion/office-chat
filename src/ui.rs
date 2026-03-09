use std::io::{self, Stdout, Write};

use crossterm::cursor::{self, MoveTo};
use crossterm::event::{KeyCode, KeyEvent};
use crossterm::style::{Color, Print, SetForegroundColor, ResetColor};
use crossterm::terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, queue};

/// A display line in the message area.
#[derive(Clone)]
pub struct DisplayLine {
    pub prefix: String,
    pub prefix_color: Color,
    pub text: String,
}

pub struct Ui {
    stdout: Stdout,
    pub input_buf: String,
    pub lines: Vec<DisplayLine>,
    pub width: u16,
    pub height: u16,
}

impl Ui {
    pub fn new() -> Self {
        let (width, height) = terminal::size().unwrap_or((80, 24));
        let mut stdout = io::stdout();
        terminal::enable_raw_mode().expect("failed to enable raw mode");
        execute!(stdout, EnterAlternateScreen, cursor::Hide).ok();
        Self {
            stdout,
            input_buf: String::new(),
            lines: Vec::new(),
            width,
            height,
        }
    }

    pub fn cleanup(&mut self) {
        terminal::disable_raw_mode().ok();
        execute!(self.stdout, cursor::Show, LeaveAlternateScreen).ok();
    }

    pub fn bell(&mut self) {
        execute!(self.stdout, Print("\x07")).ok();
    }

    pub fn push_line(&mut self, prefix: &str, prefix_color: Color, text: &str) {
        self.lines.push(DisplayLine {
            prefix: prefix.to_string(),
            prefix_color,
            text: text.to_string(),
        });
        self.render();
    }

    pub fn push_system(&mut self, text: &str) {
        self.push_line("***", Color::DarkGrey, text);
    }

    pub fn render(&mut self) {
        let (w, h) = terminal::size().unwrap_or((self.width, self.height));
        self.width = w;
        self.height = h;

        // Message area: rows 0..h-2, input line: row h-1
        let msg_rows = (h.saturating_sub(2)) as usize;
        let start = self.lines.len().saturating_sub(msg_rows);

        // Clear screen
        queue!(self.stdout, MoveTo(0, 0)).ok();

        for (i, row) in (0..msg_rows).enumerate() {
            queue!(self.stdout, MoveTo(0, row as u16), Clear(ClearType::CurrentLine)).ok();
            if let Some(line) = self.lines.get(start + i) {
                queue!(
                    self.stdout,
                    SetForegroundColor(line.prefix_color),
                    Print(&line.prefix),
                    ResetColor,
                    Print(" "),
                    Print(&line.text),
                )
                .ok();
            }
        }

        // Separator line
        let sep_row = h.saturating_sub(2);
        queue!(
            self.stdout,
            MoveTo(0, sep_row),
            Clear(ClearType::CurrentLine),
            SetForegroundColor(Color::DarkGrey),
            Print("─".repeat(w as usize)),
            ResetColor,
        )
        .ok();

        // Input line
        let input_row = h.saturating_sub(1);
        let visible_input = if self.input_buf.len() > (w as usize).saturating_sub(3) {
            &self.input_buf[self.input_buf.len() - (w as usize).saturating_sub(3)..]
        } else {
            &self.input_buf
        };
        queue!(
            self.stdout,
            MoveTo(0, input_row),
            Clear(ClearType::CurrentLine),
            SetForegroundColor(Color::Green),
            Print("> "),
            ResetColor,
            Print(visible_input),
            cursor::Show,
        )
        .ok();

        self.stdout.flush().ok();
    }

    /// Handle a keystroke. Returns Some(line) if Enter was pressed.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<String> {
        match key.code {
            KeyCode::Enter => {
                let line = self.input_buf.clone();
                self.input_buf.clear();
                self.render();
                if line.is_empty() {
                    None
                } else {
                    Some(line)
                }
            }
            KeyCode::Backspace => {
                self.input_buf.pop();
                self.render();
                None
            }
            KeyCode::Char(c) => {
                self.input_buf.push(c);
                self.render();
                None
            }
            _ => None,
        }
    }
}

impl Drop for Ui {
    fn drop(&mut self) {
        self.cleanup();
    }
}
