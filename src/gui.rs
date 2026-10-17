use eframe::egui::{self, FontId};

use crate::terminal_emulator::TerminalEmulator;

#[derive(Clone)]
struct CharSize {
    width: f32,
    height: f32,
}

pub struct TemuGui {
    terminal_emulator: TerminalEmulator,
    char_size: Option<CharSize>,
    font_id: FontId,
}

impl TemuGui {
    fn new(
        cc: &eframe::CreationContext<'_>,
        terminal_emulator: TerminalEmulator,
        text_size: f32,
    ) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Dark);

        let mut style = egui::Style::default();
        let font_id = egui::FontId::monospace(text_size);
        style.override_font_id = Some(font_id.clone());
        cc.egui_ctx.set_style_of(egui::Theme::Dark, style);

        Self {
            terminal_emulator,
            char_size: None,
            font_id,
        }
    }

    pub fn run(terminal_emulator: TerminalEmulator, text_size: f32) {
        let native_options = eframe::NativeOptions::default();
        _ = eframe::run_native(
            "Temu",
            native_options,
            Box::new(|cc| Ok(Box::new(TemuGui::new(cc, terminal_emulator, text_size)))),
        )
        .unwrap();
    }

    fn draw_cursor(&self, ui: &mut egui::Ui, origin: egui::Pos2) {
        let p = ui.painter();
        let cursor_rect = {
            let cursor = self.terminal_emulator.cursor();
            let char_size = self.char_size.clone().unwrap();
            let min = origin
                + egui::vec2(
                    cursor.col as f32 * char_size.width,
                    cursor.row as f32 * char_size.height,
                );
            let size = egui::vec2(char_size.width, char_size.height);
            egui::Rect::from_min_size(min, size)
        };

        p.rect_filled(cursor_rect, 0, egui::Color32::GRAY);
    }
}

fn get_char_size(ui: &egui::Ui, font_id: &FontId) -> CharSize {
    let pixels_per_point = ui.pixels_per_point();
    let extra_spacing = ui.spacing().extra_text_line_spacing;

    ui.fonts_mut(|fonts| {
        let width = fonts.glyph_width(&font_id, '@');
        let logical_height = fonts.row_height(&font_id) + extra_spacing;
        let height = (logical_height * pixels_per_point).round() / pixels_per_point;
        CharSize { width, height }
    })
}

impl eframe::App for TemuGui {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.char_size.is_none() {
            self.char_size = Some(get_char_size(ui, &self.font_id));
        }

        self.terminal_emulator.read();

        ui.input(|i| {
            for event in &i.events {
                let text = match event {
                    egui::Event::Text(t) => &t,
                    egui::Event::Key {
                        key: egui::Key::Enter,
                        pressed: true,
                        ..
                    } => "\r",
                    _ => continue,
                };
                self.terminal_emulator.write(text.as_bytes());
            }
        });

        egui::CentralPanel::default().show(ui, |ui| {
            let origin = ui.cursor().min;
            ui.add(egui::Label::new(self.terminal_emulator.display_text()).extend());
            self.draw_cursor(ui, origin);
        });
    }
}
