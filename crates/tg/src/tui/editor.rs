//! Grapheme-aware editing and wrapping for CJK / emoji terminals.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Default)]
pub struct Editor {
    pub text: String,
    pub cursor: usize,
}
impl Editor {
    pub fn set(&mut self, text: String) {
        self.cursor = text.len();
        self.text = text;
    }
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }
    pub fn insert(&mut self, text: &str) {
        let text = crate::output::safe(text);
        if self.text.len() + text.len() > 65_536 {
            return;
        }
        self.text.insert_str(self.cursor, &text);
        self.cursor += text.len();
    }
    fn left(&self) -> usize {
        self.text[..self.cursor]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(i, _)| i)
    }
    fn right(&self) -> usize {
        self.text[self.cursor..]
            .graphemes(true)
            .next()
            .map_or(self.cursor, |g| self.cursor + g.len())
    }
    fn vertical(&mut self, up: bool) {
        let start = self.text[..self.cursor].rfind('\n').map_or(0, |i| i + 1);
        let column = self.text[start..self.cursor].width();
        let target = if up {
            if start == 0 {
                return;
            }
            self.text[..start - 1].rfind('\n').map_or(0, |i| i + 1)
        } else {
            let Some(end) = self.text[self.cursor..].find('\n') else {
                return;
            };
            self.cursor + end + 1
        };
        let end = self.text[target..]
            .find('\n')
            .map_or(self.text.len(), |i| target + i);
        let mut width = 0;
        let mut cursor = target;
        for g in self.text[target..end].graphemes(true) {
            if width + g.width() > column {
                break;
            }
            width += g.width();
            cursor += g.len();
        }
        self.cursor = cursor;
    }
    pub fn key(&mut self, key: KeyEvent) -> bool {
        let before = self.text.clone();
        match key.code {
            KeyCode::Left => self.cursor = self.left(),
            KeyCode::Right => self.cursor = self.right(),
            KeyCode::Up => self.vertical(true),
            KeyCode::Down => self.vertical(false),
            KeyCode::Home => {
                self.cursor = self.text[..self.cursor].rfind('\n').map_or(0, |i| i + 1)
            }
            KeyCode::End => {
                self.cursor = self.text[self.cursor..]
                    .find('\n')
                    .map_or(self.text.len(), |i| self.cursor + i)
            }
            KeyCode::Backspace => {
                let from = self.left();
                self.text.drain(from..self.cursor);
                self.cursor = from;
            }
            KeyCode::Delete => {
                self.text.drain(self.cursor..self.right());
            }
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => self.cursor = 0,
            KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.cursor = self.text.len()
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.text.drain(..self.cursor);
                self.cursor = 0;
            }
            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                while self.cursor > 0
                    && self.text[..self.cursor]
                        .chars()
                        .next_back()
                        .is_some_and(char::is_whitespace)
                {
                    let from = self.left();
                    self.text.drain(from..self.cursor);
                    self.cursor = from;
                }
                while self.cursor > 0
                    && self.text[..self.cursor]
                        .chars()
                        .next_back()
                        .is_some_and(|c| !c.is_whitespace())
                {
                    let from = self.left();
                    self.text.drain(from..self.cursor);
                    self.cursor = from;
                }
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.insert(&c.to_string())
            }
            _ => {}
        }
        before != self.text
    }
    pub fn cursor_position(&self, width: usize) -> (usize, usize) {
        let lines = wrap(&self.text[..self.cursor], width.max(1));
        (
            lines
                .last()
                .map_or(0, |s| s.width())
                .min(width.saturating_sub(1)),
            lines.len().saturating_sub(1),
        )
    }
}

pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = vec![String::new()];
    let mut col = 0;
    for g in text.graphemes(true) {
        if g == "\n" || g == "\r\n" {
            lines.push(String::new());
            col = 0;
            continue;
        }
        let g = if g == "\t" { "    " } else { g };
        let w = g.width();
        if col + w > width && col > 0 {
            lines.push(String::new());
            col = 0;
        }
        lines.last_mut().unwrap().push_str(g);
        col += w;
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deletes_whole_emoji_and_combining_sequence() {
        let mut e = Editor::default();
        e.insert("中👨‍👩‍👧‍👦e\u{301}");
        e.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(e.text, "中👨‍👩‍👧‍👦");
        e.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(e.text, "中");
        e.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        e.insert("文");
        assert_eq!(e.text, "文中");
    }
    #[test]
    fn wraps_cjk_by_cells_and_preserves_newlines() {
        assert_eq!(wrap("中文ab\n第二行", 4), vec!["中文", "ab", "第二", "行"]);
    }
}
