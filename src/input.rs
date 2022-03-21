use std::collections::VecDeque;

pub struct Input {
  pub history: Vec<String>,
  pub value: String,
  pub cursor: usize,
  seq: (Option<u8>,Option<u8>,Option<u8>),
  queue: VecDeque<String>,
}

impl Default for Input {
  fn default() -> Self {
    Self {
      history: vec![],
      value: String::default(),
      cursor: 0,
      seq: (None,None,None),
      queue: VecDeque::new(),
    }
  }
}

impl Input {
  fn put_seq(&mut self, b: u8) -> bool {
    match (b, &self.seq) {
      (0x1b,(None,None,None)) => {
        self.seq.0 = Some(0x1b);
        true
      },
      (0x5b,(Some(0x1b),None,None)) => {
        self.seq.1 = Some(0x5b);
        true
      },
      (0x41,(Some(0x1b),Some(0x5b),None)) => { // up
        self.seq = (None,None,None);
        true
      },
      (0x42,(Some(0x1b),Some(0x5b),None)) => { // down
        self.seq = (None,None,None);
        true
      },
      (0x43,(Some(0x1b),Some(0x5b),None)) => { // right
        self.seq = (None,None,None);
        self.cursor = (self.cursor+1).min(self.value.len());
        true
      },
      (0x44,(Some(0x1b),Some(0x5b),None)) => { // left
        self.seq = (None,None,None);
        self.cursor = self.cursor.max(1)-1;
        true
      },
      (0x33,(Some(0x1b),Some(0x5b),None)) => {
        self.seq.2 = Some(0x33);
        true
      },
      (0x7e,(Some(0x1b),Some(0x5b),Some(0x33))) => { // delete
        self.seq = (None,None,None);
        true
      },
      _ => {
        self.seq = (None,None,None);
        false
      },
    }
  }
  pub fn putc(&mut self, b: u8) {
    if self.put_seq(b) { return }
    if b == 0x0d {
      self.queue.push_back(self.value.clone());
      self.value = String::default();
    } else if b == 0x03 { // ctrl+c
      // ...
    } else if b == 0x7f { // backspace
      self.remove_left(1);
    } else if b == 0x7e { // delete
      self.remove_right(1);
    } else if b >= 0x20 {
      self.put_bytes(&vec![b]);
    }
  }
  pub fn get_next_line(&mut self) -> Option<String> {
    self.queue.pop_front()
  }
  fn put_bytes(&mut self, buf: &[u8]) {
    let c = self.cursor.min(self.value.len());
    let s = String::from_utf8_lossy(buf);
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
