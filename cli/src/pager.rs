use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    execute, queue, terminal,
};

pub trait Content {
    fn render(&self, width: Option<usize>) -> String;

    fn update(&mut self) -> bool {
        false
    }

    fn status(&self) -> &str {
        ""
    }

    fn close_action(&self) -> &str {
        "quit"
    }
}

pub fn is_interactive() -> bool {
    io::stdout().is_terminal()
        && io::stdin().is_terminal()
        && std::env::var("TERM").is_ok_and(|term| term != "dumb")
}

struct Screen;

impl Screen {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        let guard = Self;
        execute!(io::stdout(), terminal::EnterAlternateScreen, cursor::Hide)?;
        Ok(guard)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), cursor::Show, terminal::LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

pub fn show(mut content: impl Content) -> Result<ExitCode> {
    let _screen = Screen::enter()?;
    let mut offset = 0usize;
    let mut dirty = true;
    let mut rendered = String::new();
    loop {
        let (width, height) = terminal::size()?;
        let rows = usize::from(height.saturating_sub(1)).max(1);
        if dirty {
            rendered = content.render(Some(usize::from(width)));
        }
        let lines: Vec<_> = rendered.lines().collect();
        offset = offset.min(lines.len().saturating_sub(rows));
        if dirty {
            let mut output = io::stdout().lock();
            queue!(
                output,
                cursor::MoveTo(0, 0),
                terminal::Clear(terminal::ClearType::All)
            )?;
            for (row, line) in lines.iter().skip(offset).take(rows).enumerate() {
                queue!(output, cursor::MoveTo(0, row as u16))?;
                write!(output, "{line}")?;
            }
            queue!(output, cursor::MoveTo(0, height.saturating_sub(1)))?;
            write!(
                output,
                "{}",
                console::truncate_str(
                    &format!(
                        "{}j/k line · f/b page · d/u half · g/G ends · q {}",
                        content.status(),
                        content.close_action()
                    ),
                    usize::from(width),
                    ""
                )
            )?;
            output.flush()?;
            dirty = false;
        }
        dirty |= content.update();
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            return Ok(ExitCode::SUCCESS);
                        }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            return Ok(ExitCode::from(130));
                        }
                        KeyCode::Down | KeyCode::Char('j') => offset = offset.saturating_add(1),
                        KeyCode::Up | KeyCode::Char('k') => offset = offset.saturating_sub(1),
                        KeyCode::PageDown | KeyCode::Char(' ' | 'f') => {
                            offset = offset.saturating_add(rows)
                        }
                        KeyCode::PageUp | KeyCode::Char('b') => {
                            offset = offset.saturating_sub(rows)
                        }
                        KeyCode::Char('d') => offset = offset.saturating_add(rows.div_ceil(2)),
                        KeyCode::Char('u') => offset = offset.saturating_sub(rows.div_ceil(2)),
                        KeyCode::Home | KeyCode::Char('g') => offset = 0,
                        KeyCode::End | KeyCode::Char('G') => offset = lines.len(),
                        _ => (),
                    }
                    dirty = true;
                }
                Event::Resize(_, _) => dirty = true,
                _ => (),
            }
        }
    }
}
