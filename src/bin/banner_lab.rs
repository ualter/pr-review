use std::io::{self, Write};
use std::thread;
use std::time::Duration;

const GREEN_BOLD: &str = "\x1b[1;32m";
const YELLOW_BOLD: &str = "\x1b[1;33m";
const FG_WHITE_BOLD: &str = "\x1b[1;97m";
const RESET: &str = "\x1b[0m";

fn main() {
    let logo = ["UALTER", "℗ PR-REVIEW"];

    let max_width = logo
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);

    print!("\x1b[2J\x1b[H");
    print!("\x1b[?25l");
    let _ = io::stdout().flush();

    for reveal in (0..=max_width).step_by(3) {
        print!("\x1b[H");
        print!("\n\n");

        for line in logo {
            let line_len = line.chars().count();
            let visible_len = reveal.min(line_len);
            let trailing_spaces = max_width.saturating_sub(visible_len);
            let mut rendered = String::new();

            for (i, ch) in line.chars().take(visible_len).enumerate() {
                let is_edge = i + 4 >= visible_len;
                rendered.push_str(if is_edge { FG_WHITE_BOLD } else { GREEN_BOLD });
                rendered.push(ch);
            }

            rendered.push_str(RESET);
            rendered.push_str(&" ".repeat(trailing_spaces));
            rendered.push('\n');
            print!("{rendered}");
        }

        let _ = io::stdout().flush();
        thread::sleep(Duration::from_millis(120));
    }

    let glow_window = 4usize;
    let glow_step = 1usize;
    let glow_frames = max_width + glow_window;

    for head in (0..=glow_frames).step_by(glow_step) {
        let highlight_start = head.saturating_sub(glow_window);
        let highlight_end = head;

        print!("\x1b[H");
        print!("\n\n");

        for line in logo {
            let line_len = line.chars().count();
            let mut rendered = String::new();

            for (i, ch) in line.chars().enumerate() {
                let color = if (highlight_start..highlight_end).contains(&i) {
                    YELLOW_BOLD
                } else {
                    GREEN_BOLD
                };
                rendered.push_str(color);
                rendered.push(ch);
            }

            rendered.push_str(RESET);
            rendered.push_str(&" ".repeat(max_width.saturating_sub(line_len)));
            rendered.push('\n');
            print!("{rendered}");
        }

        let _ = io::stdout().flush();
        thread::sleep(Duration::from_millis(100));
    }

    print!("\x1b[?25h");
    print!("{RESET}\n");
    let _ = io::stdout().flush();
}
