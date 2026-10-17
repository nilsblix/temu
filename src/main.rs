mod gui;
mod terminal_emulator;

use terminal_emulator::{TerminalEmulator, WindowSize};

fn main() {
    let size = WindowSize { rows: 120, cols: 80 };
    let emu = TerminalEmulator::spawn(size);
    gui::TemuGui::run(emu, 14.0);
}
