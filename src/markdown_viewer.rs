use crate::ui::{BLACK_BOLD, LINE, RESET, YELLOW_BOLD};
use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use pulldown_cmark::{CodeBlockKind, Event as MdEvent, HeadingLevel, Parser, Tag, TagEnd};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap},
    Terminal,
};
use std::{fs, io, path::Path};
use termimad::MadSkin;

pub fn open_markdown_viewer(title: &str, path: &Path) -> Result<()> {
    let markdown = fs::read_to_string(path)
        .with_context(|| format!("Failed to read markdown file: {}", path.display()))?;

    open_markdown_text(title, &markdown, false)
}

pub fn open_markdown_viewer_at_end(title: &str, path: &Path) -> Result<()> {
    let markdown = fs::read_to_string(path)
        .with_context(|| format!("Failed to read markdown file: {}", path.display()))?;

    open_markdown_text(title, &markdown, true)
}

pub fn open_markdown_text(title: &str, markdown: &str, start_at_end: bool) -> Result<()> {
    enable_raw_mode()?;

    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_viewer(&mut terminal, title, markdown, start_at_end);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_viewer(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    title: &str,
    markdown: &str,
    start_at_end: bool,
) -> Result<()> {
    let lines = render_markdown(markdown);
    let mut scroll: u16 = if start_at_end {
        let area = terminal.size()?;
        let content_height = area.height.saturating_sub(3) as usize;
        lines.len().saturating_sub(content_height) as u16
    } else {
        0
    };

    loop {
        terminal.draw(|frame| {
            let area = frame.area();

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .split(area);

            let block = Block::default()
                .title(format!(" {title} "))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray));

            let paragraph = Paragraph::new(Text::from(lines.clone()))
                .block(block)
                .scroll((scroll, 0))
                .wrap(Wrap { trim: false });

            frame.render_widget(paragraph, chunks[0]);

            let footer =
                Paragraph::new(" ↑/↓ scroll  PgUp/PgDown page  Home/End jump  q/Esc close ")
                    .style(Style::default().fg(Color::DarkGray));

            frame.render_widget(footer, chunks[1]);
        })?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Down | KeyCode::Char('j') => scroll = scroll.saturating_add(1),
                KeyCode::Up | KeyCode::Char('k') => scroll = scroll.saturating_sub(1),
                KeyCode::PageDown => scroll = scroll.saturating_add(10),
                KeyCode::PageUp => scroll = scroll.saturating_sub(10),
                KeyCode::Home => scroll = 0,
                KeyCode::End => scroll = lines.len().saturating_sub(1) as u16,
                _ => {}
            }
        }
    }

    Ok(())
}

fn render_markdown(markdown: &str) -> Vec<Line<'static>> {
    let parser = Parser::new(markdown);

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current = String::new();

    let mut in_heading: Option<HeadingLevel> = None;
    let mut in_code_block = false;
    let mut in_item = false;

    for event in parser {
        match event {
            MdEvent::Start(Tag::Heading { level, .. }) => {
                flush_line(&mut lines, &mut current);
                in_heading = Some(level);
            }

            MdEvent::End(TagEnd::Heading(_)) => {
                let text = current.trim().to_string();
                current.clear();

                lines.push(Line::from(""));

                if let Some((label, style)) = section_style(&text) {
                    lines.push(Line::from(Span::styled(label, style)));
                    lines.push(Line::from(Span::styled(
                        "────────────────────────────────────────",
                        Style::default().fg(Color::DarkGray),
                    )));
                } else {
                    lines.push(Line::from(Span::styled(
                        format_heading(in_heading, &text),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )));
                }

                lines.push(Line::from(""));
                in_heading = None;
            }

            MdEvent::Start(Tag::CodeBlock(kind)) => {
                flush_line(&mut lines, &mut current);
                in_code_block = true;

                let label = match kind {
                    CodeBlockKind::Fenced(lang) if !lang.is_empty() => {
                        format!("```{lang}")
                    }
                    _ => "```".to_string(),
                };

                lines.push(Line::from(Span::styled(
                    label,
                    Style::default().fg(Color::DarkGray),
                )));
            }

            MdEvent::End(TagEnd::CodeBlock) => {
                flush_line_styled(&mut lines, &mut current, Style::default().fg(Color::Cyan));

                lines.push(Line::from(Span::styled(
                    "```",
                    Style::default().fg(Color::DarkGray),
                )));

                lines.push(Line::from(""));
                in_code_block = false;
            }

            MdEvent::Start(Tag::Item) => {
                flush_line(&mut lines, &mut current);
                current.push_str("  • ");
                in_item = true;
            }

            MdEvent::End(TagEnd::Item) => {
                flush_line(&mut lines, &mut current);
                in_item = false;
            }

            MdEvent::Text(text) => {
                if in_code_block {
                    for line in text.lines() {
                        lines.push(Line::from(Span::styled(
                            format!("  {line}"),
                            Style::default().fg(Color::Cyan),
                        )));
                    }
                } else {
                    current.push_str(&text);
                }
            }

            MdEvent::Code(code) => {
                current.push('`');
                current.push_str(&code);
                current.push('`');
            }

            MdEvent::SoftBreak | MdEvent::HardBreak => {
                if in_code_block {
                    flush_line_styled(&mut lines, &mut current, Style::default().fg(Color::Cyan));
                } else {
                    current.push(' ');
                }
            }

            MdEvent::End(TagEnd::Paragraph) => {
                flush_line(&mut lines, &mut current);
                if !in_item {
                    lines.push(Line::from(""));
                }
            }

            MdEvent::Rule => {
                flush_line(&mut lines, &mut current);
                lines.push(Line::from(Span::styled(
                    "────────────────────────────────────────────────────────",
                    Style::default().fg(Color::DarkGray),
                )));
            }

            _ => {}
        }
    }

    flush_line(&mut lines, &mut current);

    lines
}

fn flush_line(lines: &mut Vec<Line<'static>>, current: &mut String) {
    flush_line_styled(lines, current, Style::default());
}

fn flush_line_styled(lines: &mut Vec<Line<'static>>, current: &mut String, style: Style) {
    let text = current.trim_end().to_string();

    if !text.trim().is_empty() {
        lines.push(Line::from(Span::styled(text, style)));
    }

    current.clear();
}

fn section_style(title: &str) -> Option<(String, Style)> {
    let normalized = title.to_lowercase();

    if normalized.contains("blocking issue") {
        return Some((section_label(title, "🛑  BLOCKING ISSUES"), section_red()));
    }

    if normalized.contains("non-blocking") || normalized.contains("suggestion") {
        return Some((
            section_label(title, "⚠️  NON-BLOCKING SUGGESTIONS"),
            section_yellow(),
        ));
    }

    if normalized.contains("test") {
        return Some((
            section_label(title, "🧪  TESTS TO RUN OR ADD"),
            section_cyan(),
        ));
    }

    if normalized.contains("file")
        || normalized.contains("resource")
        || normalized.contains("inspect")
    {
        return Some((
            section_label(title, "📁  FILES / RESOURCES TO INSPECT"),
            section_blue(),
        ));
    }

    if normalized.contains("deployment")
        || normalized.contains("migration")
        || normalized.contains("rollback")
    {
        return Some((
            section_label(title, "🚀  DEPLOYMENT / MIGRATION CONCERNS"),
            section_magenta(),
        ));
    }

    if normalized.contains("architecture") || normalized.contains("layering") {
        return Some((
            section_label(title, "🏗️  ARCHITECTURE / LAYERING"),
            section_magenta(),
        ));
    }

    if normalized.contains("safe") || normalized.contains("does not require changes") {
        return Some((
            section_label(title, "✅  SAFE / NO CHANGES REQUIRED"),
            section_green(),
        ));
    }

    None
}

fn section_red() -> Style {
    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
}

fn section_yellow() -> Style {
    Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD)
}

fn section_cyan() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

fn section_blue() -> Style {
    Style::default()
        .fg(Color::Blue)
        .add_modifier(Modifier::BOLD)
}

fn section_magenta() -> Style {
    Style::default()
        .fg(Color::Magenta)
        .add_modifier(Modifier::BOLD)
}

fn section_green() -> Style {
    Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD)
}

fn format_heading(level: Option<HeadingLevel>, text: &str) -> String {
    match level {
        Some(HeadingLevel::H1) => format!("📄  {text}"),
        Some(HeadingLevel::H2) => format!("◆  {text}"),
        Some(HeadingLevel::H3) => format!("›  {text}"),
        _ => text.to_string(),
    }
}

fn leading_number(title: &str) -> Option<&str> {
    title
        .split_whitespace()
        .next()
        .filter(|part| part.chars().all(|c| c.is_ascii_digit() || c == '.'))
}

fn section_label(title: &str, label: &str) -> String {
    match leading_number(title) {
        Some(number) => format!("{number} {label}"),
        None => label.to_string(),
    }
}

pub fn print_markdown_document(title: &str, path: &Path) -> Result<()> {
    let markdown = fs::read_to_string(path)
        .with_context(|| format!("Failed to read markdown file: {}", path.display()))?;

    print_markdown_text(title, &markdown)
}

pub fn print_markdown_text(title: &str, markdown: &str) -> Result<()> {
    let skin = MadSkin::default();

    println!();
    println!("{LINE}");
    println!("{YELLOW_BOLD}📄 {title}{RESET}");
    println!("{LINE}");

    skin.print_text(markdown);

    println!("{LINE}");
    println!("{BLACK_BOLD}End of document. Use your terminal scrollback to review above.{RESET}");
    println!("{LINE}");

    Ok(())
}
