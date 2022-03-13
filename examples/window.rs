use cabin::{ui,ui::UI};
use async_std::{task,sync::{Arc,Mutex}};
use signal_hook::{iterator::{SignalsInfo,exfiltrator::WithOrigin},consts::SIGWINCH};
use raw_tty::IntoRawMode;
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
      ui.add_window(vec![0;32], "one".as_bytes().to_vec());
      ui.add_window(vec![1;32], "two".as_bytes().to_vec());
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
    let mut seq = (None,None,None);
    loop {
      stdin.read_exact(&mut buf).unwrap();
      let mut ui = mui.lock().await;

      match (buf[0],seq) {
        (0x1b,(None,None,None)) => {
          seq.0 = Some(0x1b);
          continue;
        },
        (0x5b,(Some(0x1b),None,None)) => {
          seq.1 = Some(0x5b);
          continue;
        },
        (0x41,(Some(0x1b),Some(0x5b),None)) => { // up
          seq = (None,None,None);
          continue;
        },
        (0x42,(Some(0x1b),Some(0x5b),None)) => { // down
          seq = (None,None,None);
          continue;
        },
        (0x43,(Some(0x1b),Some(0x5b),None)) => { // right
          seq = (None,None,None);
          let c = (ui.cursor+1).min(ui.input.len());
          ui.set_cursor(c);
          ui.update();
          continue;
        },
        (0x44,(Some(0x1b),Some(0x5b),None)) => { // left
          seq = (None,None,None);
          let c = ui.cursor.max(1)-1;
          ui.set_cursor(c);
          ui.update();
          continue;
        },
        (0x33,(Some(0x1b),Some(0x5b),None)) => {
          seq.2 = Some(0x33);
          continue;
        },
        (0x7e,(Some(0x1b),Some(0x5b),Some(0x33))) => { // delete
          seq = (None,None,None);
          ui.remove_right(1);
          ui.update();
          continue;
        },
        _ => {
          seq = (None,None,None);
        },
      }

      if buf[0] == 0x0d {
        let parts = ui.input.split_whitespace().collect::<Vec<&str>>();
        match parts.get(0) {
          Some(&"/win") | Some(&"/w") => {
            let i: usize = parts.get(1).unwrap().parse().unwrap();
            ui.set_active_index(i);
          },
          Some(&"/quit") => break,
          _ => {},
        }
        ui.set_input("");
        ui.update();
      } else if buf[0] == 0x03 { // ctrl+c
        break;
      } else if buf[0] == 0x7f { // backspace
        ui.remove_left(1);
        ui.update();
      } else if buf[0] == 0x7e { // delete
        ui.remove_right(1);
        ui.update();
      } else if buf[0] >= 0x20 {
        ui.put(&buf);
        ui.update();
      }
    }
    print!["\x1bc"]; // reset
  });
}

fn get_size() -> ui::TermSize {
  term_size::dimensions().map(|(w,h)| (w as u32, h as u32)).unwrap()
}
async fn resizer(ui: Arc<Mutex<ui::UI>>) {
  let mut signals = SignalsInfo::<WithOrigin>::new(&vec![SIGWINCH]).unwrap();
  for info in &mut signals {
    if info.signal == SIGWINCH { ui.lock().await.resize(get_size()) }
  }
}
