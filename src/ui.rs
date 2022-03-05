use async_std::{prelude::*,task,net};
use std::collections::HashMap;
use cable::Error;
pub type Channel = Vec<u8>;
pub type Addr = Vec<u8>;
pub type TermSize = (u32,u32);

pub struct UI {
  active_window: usize,
  windows: Vec<Window>,
  diff: ansi_diff::Diff,
  size: TermSize,
}

impl UI {
  pub fn new(size: TermSize) -> Self {
    let mut windows = vec![Window::new(vec![], "!status".as_bytes().to_vec())];
    Self {
      diff: ansi_diff::Diff::new(size),
      size,
      active_window: 0,
      windows,
    }
  }
  pub fn resize(&mut self, size: TermSize) {
    self.diff.resize(size);
  }
  pub fn write_status(&mut self, msg: &str) {
    self.windows.get_mut(0).unwrap().write_ln(msg);
    self.update();
  }
  pub fn get_active(&mut self) -> (Addr,Channel) {
    let w = self.windows.get(self.active_window).unwrap();
    (w.addr.clone(), w.channel.clone())
  }
  pub fn update(&mut self) {
    let w = self.windows.get(self.active_window).unwrap();
    let update = w.lines.iter().map(|(time,line)| {
      format!["[{}] {}", timestamp(*time), line].to_string()
    }).collect::<Vec<String>>().join("\n");
    print!["{}", self.diff.update(&update)];
  }
}

fn timestamp(time: u64) -> String {
  time_format::strftime_utc("%H:%M", time as time_format::TimeStamp).unwrap()
}

pub struct Window {
  addr: Addr,
  channel: Channel,
  time_end: u64,
  limit: usize,
  lines: Vec<(u64,String)>,
}

impl Window {
  fn new(addr: Addr, channel: Channel) -> Self {
    Self {
      addr,
      channel,
      //time_start: now() - 15*60,
      time_end: 0,
      limit: 50,
      lines: vec![],
    }
  }
  pub fn write_ln(&mut self, msg: &str) {
    self.lines.push((now(), msg.to_string()));
  }
}

fn now() -> u64 {
  std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
}
