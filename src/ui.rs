use std::collections::BTreeSet;
use std::io::Write;

use cable::Channel;

use crate::{hex, input::Input};

pub type Addr = Vec<u8>;
pub type TermSize = (u32, u32);

pub struct UI {
    pub active_window: usize,
    pub active_address: Option<Addr>,
    pub windows: Vec<Window>,
    pub diff: ansi_diff::Diff,
    pub size: TermSize,
    pub input: Input,
    pub stdout: std::io::Stdout,
    tick: u64,
}

impl UI {
    pub fn new(size: TermSize) -> Self {
        let windows = vec![Window::new(vec![], "!status".to_string())];
        Self {
            diff: ansi_diff::Diff::new(size),
            size,
            active_window: 0,
            active_address: None,
            windows,
            input: Input::default(),
            stdout: std::io::stdout(),
            tick: 0,
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
        self.active_window = index.min(self.windows.len().max(1) - 1);
    }
    pub fn get_active_address<'a>(&'a self) -> Option<&'a Addr> {
        self.active_address.as_ref()
    }
    pub fn set_active_address(&mut self, addr: &Addr) {
        self.active_address = Some(addr.clone());
    }
    pub fn add_window(&mut self, address: Addr, channel: Channel) -> usize {
        self.windows.push(Window::new(address, channel));
        self.windows.len() - 1
    }
    pub fn get_window<'a>(
        &'a mut self,
        address: &Addr,
        channel: &Channel,
    ) -> Option<&'a mut Window> {
        for w in self.windows.iter_mut() {
            if &w.address == address && &w.channel == channel {
                return Some(w);
            }
        }
        return None;
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
        let mut lines = w
            .lines
            .iter()
            .map(|(time, _, line)| format!["[{}] {}", timestamp(*time), line].to_string())
            .collect::<Vec<String>>();
        for _ in lines.len()..(self.size.1 as usize) - 2 {
            lines.push(String::default());
        }
        let input = {
            let c = self.input.cursor.min(self.input.value.len());
            let n = (c + 1).min(self.input.value.len());
            let s = if n > c { &self.input.value[c..n] } else { " " };
            self.input.value[0..c].to_string() + "\x1b[7m" + s + "\x1b[0m" + &self.input.value[n..]
        };
        write![
            self.stdout,
            "{}{}",
            if self.tick == 0 { "\x1bc\x1b[?25l" } else { "" }, // clear, turn off cursor
            self.diff
                .update(&format![
                    "[{}] {}\n{}\n> {}",
                    if w.channel == "!status" {
                        w.channel.to_string()
                    } else {
                        format!["#{}", &w.channel]
                    },
                    if w.channel == "!status" && self.active_address.is_some() {
                        let addr = self.active_address.as_ref().unwrap();
                        format!["cabal://{}", hex::to(&addr)]
                    } else if w.channel == "!status" {
                        "".to_string()
                    } else {
                        format!["cabal://{}", hex::to(&w.address)]
                    },
                    lines.join("\n"),
                    &input,
                ])
                .split("\n")
                .collect::<Vec<&str>>()
                .join("\r\n"),
        ]
        .unwrap();
        self.stdout.flush().unwrap();
        self.tick += 1;
    }
    pub fn finish(&mut self) {
        write![self.stdout, "\x1bc"].unwrap();
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
    pub lines: BTreeSet<(u64, u64, String)>,
    line_index: u64,
}

impl Window {
    fn new(address: Addr, channel: Channel) -> Self {
        Self {
            address,
            channel,
            //time_start: now() - 15*60,
            time_end: 0,
            limit: 50,
            lines: BTreeSet::default(),
            line_index: 0,
        }
    }
    pub fn write(&mut self, msg: &str) {
        self.insert(now(), msg);
    }
    pub fn insert(&mut self, timestamp: u64, text: &str) {
        let index = self.line_index;
        self.line_index += 1;
        self.lines.insert((timestamp, index, text.to_string()));
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
