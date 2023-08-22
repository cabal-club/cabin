use std::{collections::BTreeSet, io::Write};

use async_std::sync::{Arc, Mutex};
use cable::Channel;
use signal_hook::{
    consts::SIGWINCH,
    iterator::{exfiltrator::WithOrigin, SignalsInfo},
};

use crate::{hex, input::Input, time};

pub type Addr = Vec<u8>;
pub type TermSize = (u32, u32);

/// Determine the dimensions of the terminal.
pub fn get_term_size() -> TermSize {
    term_size::dimensions()
        .map(|(w, h)| (w as u32, h as u32))
        .unwrap()
}

/// Resize the user interface to match the dimensions of the terminal.
pub async fn resizer(ui: Arc<Mutex<Ui>>) {
    let mut signals = SignalsInfo::<WithOrigin>::new(&vec![SIGWINCH]).unwrap();
    for info in &mut signals {
        if info.signal == SIGWINCH {
            ui.lock().await.resize(get_term_size())
        }
    }
}

/// A single user-interface window.
pub struct Window {
    /// The hex address of a cabal.
    pub address: Addr,
    /// The channel whose contents the window is displaying.
    pub channel: Channel,
    /// The age of the most recent post(s) to be displayed.
    pub time_end: u64,
    /// The total number of posts which may be displayed.
    pub limit: usize,
    /// The lines of the window (timestamp, index, text).
    pub lines: BTreeSet<(u64, u64, String)>,
    /// A line index counter to facilitate line insertions.
    line_index: u64,
}

impl Window {
    /// Create a new window with the given address and channel.
    pub fn new(address: Addr, channel: Channel) -> Self {
        Self {
            address,
            channel,
            time_end: 0,
            limit: 50,
            lines: BTreeSet::default(),
            line_index: 0,
        }
    }

    /// Write the message to the window.
    pub fn write(&mut self, msg: &str) {
        self.insert(time::now(), msg);
    }

    /// Insert a new line into the window using the given message timestamp
    /// and text.
    pub fn insert(&mut self, timestamp: u64, text: &str) {
        let index = self.line_index;
        self.line_index += 1;
        self.lines.insert((timestamp, index, text.to_string()));
    }
}

pub struct Ui {
    pub active_window: usize,
    pub active_address: Option<Addr>,
    pub windows: Vec<Window>,
    pub diff: ansi_diff::Diff,
    pub size: TermSize,
    pub input: Input,
    pub stdout: std::io::Stdout,
    tick: u64,
}

impl Ui {
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

    pub fn get_active_window(&mut self) -> &mut Window {
        self.windows.get_mut(self.active_window).unwrap()
    }

    pub fn get_active_index(&self) -> usize {
        self.active_window
    }

    pub fn set_active_index(&mut self, index: usize) {
        self.active_window = index.min(self.windows.len().max(1) - 1);
    }

    pub fn get_active_address(&self) -> Option<&Addr> {
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
        self.windows
            .iter_mut()
            .find(|w| &w.address == address && &w.channel == channel)
    }

    pub fn get_window_index(&self, address: &Addr, channel: &Channel) -> Option<usize> {
        self.windows
            .iter()
            .position(|w| &w.address == address && &w.channel == channel)
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
        // Get the active window.
        let window = self.windows.get(self.active_window).unwrap();

        let mut lines = window
            .lines
            .iter()
            .map(|(time, _, line)| format!["[{}] {}", time::timestamp(*time), line])
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
                    if window.channel == "!status" {
                        window.channel.to_string()
                    } else {
                        format!["#{}", &window.channel]
                    },
                    if window.channel == "!status" && self.active_address.is_some() {
                        let addr = self.active_address.as_ref().unwrap();
                        format!["cabal://{}", hex::to(addr)]
                    } else if window.channel == "!status" {
                        "".to_string()
                    } else {
                        format!["cabal://{}", hex::to(&window.address)]
                    },
                    lines.join("\n"),
                    &input,
                ])
                .split('\n')
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
