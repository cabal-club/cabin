use async_std::{prelude::*,task,net};
use std::collections::HashMap;
use cable::Error;
pub type Channel = Vec<u8>;
pub type Addr = Vec<u8>;

pub struct UI {
  active_window: usize,
  windows: Vec<Window>,
}

impl Default for UI {
  fn default() -> Self {
    let mut windows = vec![Window::new(vec![], "!status".as_bytes().to_vec())];
    Self {
      active_window: 0,
      windows,
    }
  }
}

impl UI {
  pub fn write_status(&mut self, msg: &str) {
    self.windows.get_mut(0).unwrap().write_ln(msg);
  }
  pub fn get_active(&mut self) -> (Addr,Channel) {
    let w = self.windows.get(self.active_window).unwrap();
    (w.addr.clone(), w.channel.clone())
  }
}

pub struct Window {
  addr: Addr,
  channel: Channel,
  time_start: u64,
  time_end: u64,
  limit: usize,
  lines: Vec<String>,
}

impl Window {
  fn new(addr: Addr, channel: Channel) -> Self {
    Self {
      addr,
      channel,
      time_start: now() - 15*60,
      time_end: 0,
      limit: 50,
      lines: vec![],
    }
  }
  pub fn write_ln(&mut self, msg: &str) {
    self.lines.extend(msg.split('\n').map(|s| s.to_string()));
  }
}

fn now() -> u64 {
  std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
}
