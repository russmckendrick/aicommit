//! A small multi-line text editor for commit messages: insert, delete,
//! newline, arrow keys, Home/End, and word-wise Alt+Backspace.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Default)]
pub struct TextEditor {
    lines: Vec<String>,
    row: usize,
    col: usize,
}

impl TextEditor {
    pub fn new(text: &str) -> Self {
        let mut editor = Self::default();
        editor.set_text(text);
        editor
    }

    pub fn set_text(&mut self, text: &str) {
        self.lines = text.lines().map(str::to_owned).collect();
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.row = self.lines.len() - 1;
        self.col = self.char_len(self.row);
    }

    pub fn text(&self) -> String {
        self.lines.join("\n").trim_end().to_owned()
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn cursor(&self) -> (usize, usize) {
        (self.row, self.col)
    }

    pub fn is_empty(&self) -> bool {
        self.lines.iter().all(|line| line.trim().is_empty())
    }

    fn char_len(&self, row: usize) -> usize {
        self.lines[row].chars().count()
    }

    fn byte_index(&self, row: usize, col: usize) -> usize {
        self.lines[row]
            .char_indices()
            .nth(col)
            .map_or(self.lines[row].len(), |(index, _)| index)
    }

    /// Apply a key press. Returns true when the text changed.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let index = self.byte_index(self.row, self.col);
                self.lines[self.row].insert(index, ch);
                self.col += 1;
                true
            }
            KeyCode::Enter => {
                let index = self.byte_index(self.row, self.col);
                let rest = self.lines[self.row].split_off(index);
                self.lines.insert(self.row + 1, rest);
                self.row += 1;
                self.col = 0;
                true
            }
            KeyCode::Backspace if key.modifiers.contains(KeyModifiers::ALT) => {
                let mut changed = false;
                while self.col > 0 {
                    let prev = self.lines[self.row].chars().nth(self.col - 1);
                    if changed && prev.is_some_and(char::is_whitespace) {
                        break;
                    }
                    changed |= self.backspace();
                }
                changed
            }
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => {
                if self.col < self.char_len(self.row) {
                    let index = self.byte_index(self.row, self.col);
                    self.lines[self.row].remove(index);
                    true
                } else if self.row + 1 < self.lines.len() {
                    let next = self.lines.remove(self.row + 1);
                    self.lines[self.row].push_str(&next);
                    true
                } else {
                    false
                }
            }
            KeyCode::Left => {
                if self.col > 0 {
                    self.col -= 1;
                } else if self.row > 0 {
                    self.row -= 1;
                    self.col = self.char_len(self.row);
                }
                false
            }
            KeyCode::Right => {
                if self.col < self.char_len(self.row) {
                    self.col += 1;
                } else if self.row + 1 < self.lines.len() {
                    self.row += 1;
                    self.col = 0;
                }
                false
            }
            KeyCode::Up => {
                if self.row > 0 {
                    self.row -= 1;
                    self.col = self.col.min(self.char_len(self.row));
                }
                false
            }
            KeyCode::Down => {
                if self.row + 1 < self.lines.len() {
                    self.row += 1;
                    self.col = self.col.min(self.char_len(self.row));
                }
                false
            }
            KeyCode::Home => {
                self.col = 0;
                false
            }
            KeyCode::End => {
                self.col = self.char_len(self.row);
                false
            }
            _ => false,
        }
    }

    fn backspace(&mut self) -> bool {
        if self.col > 0 {
            let index = self.byte_index(self.row, self.col - 1);
            self.lines[self.row].remove(index);
            self.col -= 1;
            true
        } else if self.row > 0 {
            let line = self.lines.remove(self.row);
            self.row -= 1;
            self.col = self.char_len(self.row);
            self.lines[self.row].push_str(&line);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn edits_multibyte_text_and_lines() {
        let mut editor = TextEditor::new("✨ feat");
        editor.handle_key(key(KeyCode::Char('!')));
        assert_eq!(editor.text(), "✨ feat!");
        editor.handle_key(key(KeyCode::Home));
        editor.handle_key(key(KeyCode::Right));
        editor.handle_key(key(KeyCode::Backspace));
        assert_eq!(editor.text(), " feat!");
        editor.handle_key(key(KeyCode::End));
        editor.handle_key(key(KeyCode::Enter));
        editor.handle_key(key(KeyCode::Enter));
        editor.handle_key(key(KeyCode::Char('-')));
        assert_eq!(editor.text(), " feat!\n\n-");
        assert_eq!(editor.cursor(), (2, 1));
        editor.handle_key(key(KeyCode::Up));
        editor.handle_key(key(KeyCode::Up));
        editor.handle_key(key(KeyCode::Delete));
        assert_eq!(editor.lines()[0], "feat!");
    }

    #[test]
    fn alt_backspace_deletes_a_word() {
        let mut editor = TextEditor::new("fix: handle errors");
        editor.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::ALT));
        assert_eq!(editor.text(), "fix: handle");
    }
}
