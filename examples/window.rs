use async_std::{
    sync::{Arc, Mutex},
    task,
};
use cabin::ui::{TermSize, UI};
use raw_tty::IntoRawMode;
use signal_hook::{
    consts::SIGWINCH,
    iterator::{exfiltrator::WithOrigin, SignalsInfo},
};
use std::io::Read;

fn main() {
    let mui = Arc::new(Mutex::new(UI::new(get_size())));
    {
        let ui = mui.clone();
        task::spawn(async move { resizer(ui).await });
    }
    {
        let muic = mui.clone();
        task::block_on(async move {
            let mut ui = muic.lock().await;
            ui.add_window(vec![0; 32], "one".as_bytes().to_vec());
            ui.add_window(vec![1; 32], "two".as_bytes().to_vec());
            ui.write(1, "test line 1");
            ui.write(1, "test line 2");
            ui.write(1, "test line 3");
            ui.write(1, "test line 4");
            ui.write(1, "test line 5");
            ui.write(2, "AAAAAAAAA");
            ui.write(2, "BBBBBBBBBBBBBBBBBb");
            ui.write(2, "CCCCC");
            ui.write(2, "DDDDDDDDDDDDD");
            ui.write(2, "EEEEEEEEEEEEEEEEEEEEe");
            ui.write(2, "FFFFFFFFF");
            ui.update();
        });
    }
    task::block_on(async move {
        let mut stdin = std::io::stdin().into_raw_mode().unwrap();
        let mut buf = vec![0];
        let mut exit = false;
        while !exit {
            stdin.read_exact(&mut buf).unwrap();
            let mut ui = mui.lock().await;
            ui.input.putc(buf[0]);
            while let Some(line) = ui.input.get_next_line() {
                let parts = line.split_whitespace().collect::<Vec<&str>>();
                match parts.get(0) {
                    Some(&"/win") | Some(&"/w") => {
                        let i: usize = parts.get(1).unwrap().parse().unwrap();
                        ui.set_active_index(i);
                    }
                    Some(&"/quit") | Some(&"/exit") => {
                        exit = true;
                        break;
                    }
                    _ => {}
                }
            }
            ui.update();
        }
        print!["\x1bc"]; // reset
    });
}

fn get_size() -> TermSize {
    term_size::dimensions()
        .map(|(w, h)| (w as u32, h as u32))
        .unwrap()
}
async fn resizer(ui: Arc<Mutex<UI>>) {
    let mut signals = SignalsInfo::<WithOrigin>::new(&vec![SIGWINCH]).unwrap();
    for info in &mut signals {
        if info.signal == SIGWINCH {
            ui.lock().await.resize(get_size())
        }
    }
}
