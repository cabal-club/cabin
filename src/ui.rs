pub type Channel = Vec<u8>;
pub type Addr = Vec<u8>;
pub type TermSize = (u32,u32);

pub struct UI {
  pub active_window: usize,
  pub windows: Vec<Window>,
  pub diff: ansi_diff::Diff,
  pub size: TermSize,
}

impl UI {
  pub fn new(size: TermSize) -> Self {
    let windows = vec![Window::new(vec![], "!status".as_bytes().to_vec())];
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
  pub fn get_size(&self) -> TermSize {
    self.size
  }
  pub fn write_status(&mut self, msg: &str) {
    self.windows.get_mut(0).unwrap().write(msg);
  }
  pub fn write(&mut self, index: usize, msg: &str) {
    self.windows.get_mut(index).unwrap().write(msg);
  }
  pub fn get_active_window<'a>(&'a mut self) -> &'a mut Window {
    self.windows.get_mut(self.active_window).unwrap()
  }
  pub fn get_active_index(&self) -> usize {
    self.active_window
  }
  pub fn set_active_index(&mut self, index: usize) {
    self.active_window = index.min(self.windows.len().max(1)-1);
  }
  pub fn add_window(&mut self, address: Addr, channel: Channel) -> usize {
    self.windows.push(Window::new(address, channel));
    self.windows.len() - 1
  }
  pub fn move_window(&mut self, src: usize, dst: usize) {
    let w = self.windows.remove(src);
    self.windows.insert(dst, w);
  }
  pub fn remove_window(&mut self, index: usize) {
    self.windows.remove(index);
    if index < self.active_window {
      self.active_window = self.active_window.min(1) - 1;
    }
  }
  pub fn update(&mut self) {
    let w = self.windows.get(self.active_window).unwrap();
    let lines = w.lines.iter().map(|(time,line)| {
      format!["[{}] {}", timestamp(*time), line].to_string()
    }).collect::<Vec<String>>().join("\n");
    print!["{}", self.diff.update(&format![indoc::indoc![r#"
      CABIN {}
      {}
    "#], to_hex(&w.address), lines])];
  }
}

fn timestamp(time: u64) -> String {
  time_format::strftime_utc("%H:%M", time as time_format::TimeStamp).unwrap()
}

pub struct Window {
  pub address: Addr,
  pub channel: Channel,
  pub time_end: u64,
  pub limit: usize,
  pub lines: Vec<(u64,String)>,
}

impl Window {
  fn new(address: Addr, channel: Channel) -> Self {
    Self {
      address,
      channel,
      //time_start: now() - 15*60,
      time_end: 0,
      limit: 50,
      lines: vec![],
    }
  }
  pub fn write(&mut self, msg: &str) {
    self.lines.push((now(), msg.to_string()));
  }
}

fn now() -> u64 {
  std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
}

fn to_hex(addr: &Addr) -> String {
  addr.iter().map(|byte| format!["{:x}", byte]).collect::<Vec<String>>().join("")
}
