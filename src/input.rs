use std::collections::VecDeque;
use terminal_keycode::{Decoder,KeyCode};

pub struct Input {
  pub history: Vec<String>,
  pub value: String,
  pub cursor: usize,
  decoder: Decoder,
  queue: VecDeque<InputEvent>,
}

pub enum InputEvent {
  Line(String),
  KeyCode(KeyCode),
}

impl Default for Input {
  fn default() -> Self {
    Self {
      history: vec![],
      value: String::default(),
      cursor: 0,
      decoder: Decoder::default(),
      queue: VecDeque::new(),
    }
  }
}

impl Input {
  pub fn putc(&mut self, b: u8) {
    for keycode in self.decoder.write(b) {
      match keycode {
        KeyCode::Enter | KeyCode::Linefeed => {
          self.queue.push_back(InputEvent::Line(self.value.clone()));
          self.value = String::default();
        },
        KeyCode::Backspace | KeyCode::CtrlH => {
          self.remove_left(1);
        },
        KeyCode::Delete => {
          self.remove_right(1);
        },
        KeyCode::ArrowLeft => {
          self.cursor = self.cursor.max(1)-1;
        },
        KeyCode::ArrowRight => {
          self.cursor = (self.cursor+1).min(self.value.len());
        },
        KeyCode::Home => {
          self.cursor = 0;
        },
        KeyCode::End => {
          self.cursor = self.value.len();
        },
        code => {
          if let Some(c) = code.printable() {
            self.put_str(&c.to_string());
          } else {
            self.queue.push_back(InputEvent::KeyCode(code));
          }
        }
      }
    }
  }
  pub fn next(&mut self) -> Option<InputEvent> {
    self.queue.pop_front()
  }
  fn put_str(&mut self, s: &str) {
    let c = self.cursor.min(self.value.len());
    self.value = self.value[0..c].to_string() + &s + &self.value[c..];
    self.cursor = (self.cursor+1).min(self.value.len());
  }
  pub fn set_value(&mut self, input: &str) {
    self.value = input.to_string();
    self.cursor = self.cursor.min(self.value.len());
  }
  pub fn remove_left(&mut self, n: usize) {
    let len = self.value.len();
    let c = self.cursor;
    self.value = self.value[0..c.max(n)-n].to_string() + &self.value[c.min(len)..];
    self.cursor = self.cursor.max(n) - n;
  }
  pub fn remove_right(&mut self, n: usize) {
    let len = self.value.len();
    let c = self.cursor;
    self.value = self.value[0..c].to_string() + &self.value[(c+n).min(len)..];
  }
  pub fn set_cursor(&mut self, cursor: usize) {
    self.cursor = cursor;
  }
}
