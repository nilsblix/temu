use std::os::fd::OwnedFd;

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

                match nix::unistd::execvp(&shell_name, &[shell_name]) {
                    Ok(infallible) => match infallible {},
                    Err(e) => {
                        println!("could not spawn shell: {e}");
                        std::process::exit(127);
                    }
                }
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
        "Temu",
        native_options,
        Box::new(|cc| Ok(Box::new(Temu::new(cc, fd, 16.0)))),
    )
    .unwrap();
}

/// 1-indexed.
struct CursorPos {
    row: usize,
    col: usize,
}

impl Default for CursorPos {
    fn default() -> Self {
        Self { row: 1, col: 1 }
    }
}

#[derive(Clone)]
struct CharSize {
    width: f32,
    height: f32,
}

struct Temu {
    fd: OwnedFd,
    buf: Vec<u8>,
    cursor: CursorPos,
    char_size: Option<CharSize>,
    font_id: egui::FontId,
}

impl Temu {
    fn new(cc: &eframe::CreationContext<'_>, fd: OwnedFd, text_size: f32) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Dark);

        let mut style = egui::Style::default();
        let font_id = egui::FontId::monospace(text_size);
        style.override_font_id = Some(font_id.clone());
        cc.egui_ctx.set_style_of(egui::Theme::Dark, style);

        Self {
            fd,
            buf: Vec::new(),
            cursor: CursorPos::default(),
            char_size: None,
            font_id,
        }
    }

    fn write_bytes(&self, bytes: &[u8]) {
        let mut to_write = bytes;
        while to_write.len() > 0 {
            match nix::unistd::write(&self.fd, bytes) {
                Ok(n) => to_write = &to_write[n..],
                Err(e) => println!("failed to write to shell: {e}"),
            }
        }
    }

    fn read_bytes(&self) -> Vec<u8> {
        let mut incoming = [0u8; 4096];
        match nix::unistd::read(&self.fd, &mut incoming) {
            Ok(n) => {
                let mut v = Vec::new();
                v.extend_from_slice(&incoming[0..n]);
                return v;
            }
            Err(Errno::EAGAIN) => {}
            Err(e) => {
                println!("failed to read from shell: {e}");
            }
        }

        Vec::new()
    }
}

impl eframe::App for Temu {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.char_size.is_none() {
            self.char_size = Some(ui.fonts_mut(|fonts| {
                let width = fonts.glyph_width(&self.font_id, '@');
                let height = fonts.row_height(&self.font_id);
                CharSize { width, height }
            }));
        }

        let mut incoming = self.read_bytes();
        for b in &incoming {
            match *b {
                b'\n' => {
                    self.cursor.row += 1;
                    self.cursor.col = 1;
                }
                _ => self.cursor.col += 1,
            }
        }
        self.buf.append(&mut incoming);

        ui.input(|i| {
            for event in &i.events {
                let text = match event {
                    egui::Event::Text(t) => t.as_str(),
                    egui::Event::Key {
                        key: egui::Key::Enter,
                        pressed: true,
                        ..
                    } => "\r",
                    _ => continue,
                };
                self.write_bytes(text.as_bytes());
            }
        });

        egui::CentralPanel::default().show(ui, |ui| {
            let origin = ui.cursor().min;

            let s = unsafe { std::str::from_utf8_unchecked(&self.buf) };
            ui.label(s);

            let p = ui.painter();
            let (font_width, font_height) = p.fonts_mut(|f| {
                let w = f.glyph_width(&egui::FontId::monospace(16.0), 'a');
                let h = f.row_height(&egui::FontId::monospace(16.0));
                (w, h)
            });

            let cursor_rect = {
                let char_size = self.char_size.clone().unwrap();
                let min = origin + egui::vec2(
                    (self.cursor.col - 1) as f32 * char_size.width,
                    (self.cursor.row - 1) as f32 * char_size.height,
                );
                egui::Rect::from_min_size(min, egui::vec2(font_width, font_height))
            };

            p.rect_filled(cursor_rect, 0, egui::Color32::GRAY);
        });
    }
}
