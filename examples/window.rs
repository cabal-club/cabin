//! Creates two UI windows and writes text to each.
//!
//! Enter `/win 1` or `/win 2` to switch between the two windows.
//!
//! Enter `/quit` to exit the application.

use std::io::Read;

use async_std::{
    sync::{Arc, Mutex},
    task,
};
use cabin::{input::InputEvent, ui, ui::Ui};
use raw_tty::IntoRawMode;

fn main() {
    // Create a new instance of the UI, sized to the terminal dimensions.
    let ui = Arc::new(Mutex::new(Ui::new(ui::get_term_size())));

    let ui_clone = ui.clone();
    // Resize the UI if the size of the terminal changes.
    task::spawn(async move { ui::resizer(ui_clone).await });

    let ui_clone = ui.clone();
    // Add two UI windows and write text to both.
    task::block_on(async move {
        let mut ui = ui_clone.lock().await;
        ui.add_window(vec![0; 32], "one".to_string());
        ui.add_window(vec![1; 32], "two".to_string());
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

    task::block_on(async move {
        let mut stdin = std::io::stdin().into_raw_mode().unwrap();
        let mut buf = vec![0];
        let mut exit = false;

        while !exit {
            let mut ui = ui.lock().await;
            // Read the input from stdin.
            stdin.read_exact(&mut buf).unwrap();
            // Collect input.
            let lines = {
                ui.input.putc(buf[0]);
                ui.update();
                let mut lines = vec![];
                while let Some(event) = ui.input.next_event() {
                    if let InputEvent::Line(line) = event {
                        lines.push(line);
                    }
                }
                lines
            };
            // Parse the input.
            for line in lines {
                let parts = line.split_whitespace().collect::<Vec<&str>>();
                match parts.get(0) {
                    // Switch window based on input.
                    Some(&"/win") | Some(&"/w") => {
                        let i: usize = parts.get(1).unwrap().parse().unwrap();
                        ui.set_active_index(i);
                    }
                    // Quit the application.
                    Some(&"/quit") | Some(&"/exit") => {
                        exit = true;
                        break;
                    }
                    _ => {}
                }
            }

            ui.update();
        }

        let mut ui = ui.lock().await;
        ui.finish();
    });
}
