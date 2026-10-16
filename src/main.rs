use std::{os::fd::OwnedFd, process::Command};

use eframe::egui;
use nix::errno::Errno;
use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::pty::{ForkptyResult, forkpty};

fn spawn_pty_with_shell() -> OwnedFd {
    unsafe {
        match forkpty(None, None) {
            Ok(ForkptyResult::Parent { master, child: _ }) => {
                let flags = OFlag::from_bits_truncate(fcntl(&master, FcntlArg::F_GETFL).unwrap());
                fcntl(&master, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK)).unwrap();
                master
            }
            Ok(ForkptyResult::Child) => {
                let shell_name = c"bash";
                nix::unistd::execvp(&shell_name, &[shell_name]).expect("could not launch shell");
                std::process::exit(0);
            }
            Err(e) => {
                panic!("error: could not fork into pty: {e}");
            }
        }
    }
}

fn main() {
    let fd = spawn_pty_with_shell();
    let native_options = eframe::NativeOptions::default();
    _ = eframe::run_native(
        "Thad",
        native_options,
        Box::new(|cc| Ok(Box::new(Thad::new(cc, fd)))),
    )
    .unwrap();
}

struct Thad {
    fd: OwnedFd,
    buf: Vec<u8>,
}

impl Thad {
    fn new(cc: &eframe::CreationContext<'_>, fd: OwnedFd) -> Self {
        // Customize egui here with cc.egui_ctx.set_fonts and
        // cc.egui_ctx.set_global_style. Restore app state using cc.storage
        // (requires the "persistence" feature). Use the cc.gl (a glow::Context)
        // to create graphics shaders and buffers that you can use for e.g.
        // egui::PaintCallback.
        Self {
            fd,
            buf: Vec::new(),
        }
    }
}

impl eframe::App for Thad {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        ui.set_theme(egui::Theme::Dark);

        let mut incoming = [0u8; 4096];
        match nix::unistd::read(&self.fd, &mut incoming) {
            Ok(n) => self.buf.extend_from_slice(&incoming[0..n]),
            Err(Errno::EAGAIN) => {}
            Err(e) => {
                eprintln!("could not read from shell: {e}");
                return;
            }
        };

        egui::CentralPanel::default().show(ui, |ui| {
            let s = unsafe { std::str::from_utf8_unchecked(&self.buf) };
            ui.monospace(s);
        });
    }
}
