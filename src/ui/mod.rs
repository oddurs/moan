//! Terminal setup and the input loop.

pub mod app;
pub mod markdown;
pub mod panel;
pub mod render;
pub mod theme;

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event as Term, EventStream, KeyCode, KeyEvent,
    KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use crossterm::execute;
use tokio::time::{interval, MissedTickBehavior};
use tokio_stream::StreamExt;

use app::{App, Mode};

pub async fn run(app: &mut App) -> Result<()> {
    let mut term = ratatui::init();
    execute!(std::io::stdout(), EnableMouseCapture)?;
    let out = drive(&mut term, app).await;
    // Where they stopped, so the next run starts there.
    app.remember_place();
    // The write-ahead log only shrinks when somebody checkpoints it.
    app.store.checkpoint();
    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    out
}

async fn drive(term: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    let mut input = EventStream::new();
    let mut tick = interval(Duration::from_millis(app.cfg.ui.poll_ms.max(100)));
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        if app.dirty {
            term.draw(|f| render::draw(f, app))?;
            app.dirty = false;
        }
        if app.quit {
            return Ok(());
        }

        tokio::select! {
            Some(Ok(ev)) = input.next() => {
                match ev {
                    Term::Key(k) if k.kind == KeyEventKind::Press => key(app, k)?,
                    Term::Mouse(m) => mouse(app, m),
                    Term::Resize(..) => app.dirty = true,
                    _ => {}
                }
            }
            Some(d) = app.done.recv() => {
                app.absorb(d);
                // Drain the rest of the burst before redrawing.
                while let Ok(d) = app.done.try_recv() {
                    app.absorb(d);
                }
            }
            _ = tick.tick() => {
                app.tick = app.tick.wrapping_add(1);
                // The working indicator animates and the freshness accent
                // fades, so time passing is itself a reason to redraw — but
                // only while either has something left to show.
                // Something is turning on screen while any of these hold.
                if app.awaiting() || app.fading() || app.pending > 0 {
                    app.dirty = true;
                }
                app.settle();
                // A few seconds apart: another moan sharing this archive may
                // have condensed something we are still showing plain.
                if app.tick.is_multiple_of(12) {
                    app.sweep();
                }
                if let Err(e) = app.poll() {
                    app.status = format!("read failed: {e}");
                    app.status_is_error = true;
                    app.dirty = true;
                }
            }
        }
    }
}

fn key(app: &mut App, k: KeyEvent) -> Result<()> {
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);

    match app.mode {
        Mode::Filter => {
            match k.code {
                KeyCode::Esc => {
                    app.filter.clear();
                    app.mode = Mode::Feed;
                    app.rebuild();
                }
                KeyCode::Enter => app.mode = Mode::Feed,
                KeyCode::Backspace => {
                    app.filter.pop();
                    app.rebuild();
                }
                KeyCode::Char(c) => {
                    app.filter.push(c);
                    app.rebuild();
                }
                _ => {}
            }
            app.dirty = true;
            return Ok(());
        }
        Mode::Help => {
            app.mode = Mode::Feed;
            app.dirty = true;
            return Ok(());
        }
        Mode::Feed => {}
    }

    use crate::ui::panel::Focus;

    // With the panel focused, movement and Enter act on it. Everything else
    // falls through to the feed, so `o`, `t`, `a` and the rest keep working
    // wherever the keyboard happens to be.
    // Typing into the panel's search box takes every printable key, so it is
    // handled before anything else can claim one.
    if app.panel.open && app.panel.focus == Focus::Panel && app.panel.typing {
        match k.code {
            KeyCode::Esc => {
                // Stop typing; a second escape puts the list back.
                if app.panel.typing && !app.panel.query.is_empty() {
                    app.panel.typing = false;
                } else {
                    app.panel.query.clear();
                    app.panel.typing = false;
                    app.rebuild_panel();
                }
            }
            KeyCode::Enter => {
                app.panel.typing = false;
                app.panel_enter()?;
            }
            KeyCode::Backspace => {
                app.panel.query.pop();
                app.rebuild_panel();
            }
            KeyCode::Down | KeyCode::Up => {
                app.panel.typing = false;
                app.panel.step(k.code == KeyCode::Down);
            }
            KeyCode::Char(c) => {
                app.panel.query.push(c);
                app.rebuild_panel();
            }
            _ => {}
        }
        app.dirty = true;
        return Ok(());
    }

    if app.panel.open && app.panel.focus == Focus::Panel {
        match k.code {
            KeyCode::Up | KeyCode::Char('k') => {
                app.panel.step(false);
                app.dirty = true;
                return Ok(());
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.panel.step(true);
                app.dirty = true;
                return Ok(());
            }
            KeyCode::Enter => {
                app.panel_enter()?;
                return Ok(());
            }
            KeyCode::Esc if !app.panel.query.is_empty() => {
                app.panel.query.clear();
                app.rebuild_panel();
                app.dirty = true;
                return Ok(());
            }
            KeyCode::Esc => {
                // Hand the keyboard back without closing: you are still
                // looking at the list, you just want to read again.
                app.panel.focus = Focus::Feed;
                app.dirty = true;
                return Ok(());
            }
            // In the panel, `/` is the panel's search, not the feed's filter.
            // In the list it narrows what is there; in search it asks the
            // archive. Both are finding, so both are `/`.
            KeyCode::Char('/') => {
                app.panel.typing = true;
                app.rebuild_panel();
                app.dirty = true;
                return Ok(());
            }
            KeyCode::Char('\t') => {}
            KeyCode::Char('h' | 'l') | KeyCode::Left | KeyCode::Right => {
                let forward = matches!(k.code, KeyCode::Char('l') | KeyCode::Right);
                let to = app.panel.mode.step(forward);
                app.panel.switch(to);
                app.rebuild_panel();
                app.dirty = true;
                return Ok(());
            }
            _ => {}
        }
    }

    match k.code {
        KeyCode::Char('q') | KeyCode::Esc if app.filter.is_empty() => app.quit = true,
        KeyCode::Esc => {
            app.filter.clear();
            app.rebuild();
            app.dirty = true;
        }
        KeyCode::Char('c') if ctrl => app.quit = true,

        KeyCode::Up | KeyCode::Char('k') => app.move_by(-1),
        KeyCode::Down | KeyCode::Char('j') => app.move_by(1),
        KeyCode::PageUp => app.move_by(-20),
        KeyCode::PageDown => app.move_by(20),
        KeyCode::Char('u') if ctrl => app.move_by(-10),
        KeyCode::Char('d') if ctrl => app.move_by(10),
        KeyCode::Char('g') | KeyCode::Home => app.go_top(),
        KeyCode::Char('G') | KeyCode::End => app.go_bottom(),

        KeyCode::Enter | KeyCode::Char(' ') => app.toggle_expand(),

        KeyCode::Char('f') => {
            app.follow = !app.follow;
            if app.follow {
                app.go_bottom();
            }
            app.dirty = true;
        }
        KeyCode::Char('t') => {
            app.cfg.ui.show_thinking = !app.cfg.ui.show_thinking;
            app.rebuild();
            app.dirty = true;
        }
        KeyCode::Char('o') => {
            app.cfg.ui.show_tools = !app.cfg.ui.show_tools;
            app.rebuild();
            app.dirty = true;
        }
        KeyCode::Char('/') => {
            app.mode = Mode::Filter;
            app.dirty = true;
        }
        KeyCode::Char('s') => app.toggle_panel()?,
        KeyCode::Char('r') => {
            app.recondense();
            app.dirty = true;
        }
        KeyCode::Char('R') => {
            match app.store.clear_gists() {
                Ok(n) => {
                    app.status = format!("dropped {n} cached condensations");
                    app.status_is_error = false;
                    if let Some(s) = app.session.clone() {
                        app.events = app.store.load(&s.id)?;
                    }
                    app.forget_queued();
                    app.retry_pending();
                    app.rebuild();
                }
                Err(e) => {
                    app.status = format!("{e}");
                    app.status_is_error = true;
                }
            }
            app.dirty = true;
        }
        // Open, tab is what moves between panes; closed, it still cycles a
        // project's sessions, which is the job the panel took over.
        KeyCode::Tab | KeyCode::BackTab if app.panel.open => {
            app.panel.focus = match app.panel.focus {
                Focus::Feed => Focus::Panel,
                Focus::Panel => Focus::Feed,
            };
            app.dirty = true;
        }
        KeyCode::Tab => app.cycle(1)?,
        KeyCode::BackTab => app.cycle(-1)?,
        KeyCode::Char('O') => app.open_link(),
        KeyCode::Char('a') => {
            let on = !app.merged;
            app.set_merged(on)?;
        }
        KeyCode::Char(c @ '1'..='9') => app.jump(c as usize - '1' as usize)?,
        KeyCode::Char('?') => {
            app.mode = Mode::Help;
            app.dirty = true;
        }
        _ => {}
    }
    Ok(())
}

/// A wheel event at a column, for tests.
#[cfg(test)]
pub fn wheel_for_test(app: &mut App, column: u16, down: bool) {
    mouse(
        app,
        crossterm::event::MouseEvent {
            kind: if down {
                MouseEventKind::ScrollDown
            } else {
                MouseEventKind::ScrollUp
            },
            column,
            row: 5,
            modifiers: KeyModifiers::NONE,
        },
    );
}

/// A left click at a cell, for tests.
#[cfg(test)]
pub fn click_for_test(app: &mut App, column: u16, row: u16) {
    mouse(
        app,
        crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        },
    );
}

fn mouse(app: &mut App, m: crossterm::event::MouseEvent) {
    use crate::ui::panel::{Focus, Row as PRow};
    if app.mode != Mode::Feed {
        return;
    }

    // Whatever is under the pointer, whether or not it has the keyboard. A
    // wheel that scrolls the other pane is a wheel that feels broken.
    let split = app.panel.splits(app.width);
    let over_panel = split && m.column < app.panel.columns(app.width);
    let on_divider = split && m.column == app.panel.columns(app.width);

    if on_divider && matches!(m.kind, MouseEventKind::Drag(MouseButton::Left)) {
        app.panel.width = m
            .column
            .clamp(crate::ui::panel::MIN_WIDTH, crate::ui::panel::MAX_WIDTH);
        app.invalidate_layout();
        app.dirty = true;
        return;
    }

    if over_panel {
        match m.kind {
            MouseEventKind::ScrollUp => app.panel.scroll_by(-3),
            MouseEventKind::ScrollDown => app.panel.scroll_by(3),
            MouseEventKind::Down(MouseButton::Left) => {
                // The header is pinned above the list: one row for the tabs
                // and a blank, plus two more when the query line is showing.
                let lead = if app.panel.mode.takes_input() || !app.panel.query.is_empty() {
                    4
                } else {
                    2
                };
                // A click is measured from whatever the list is showing first.
                let first = app.panel.first_shown();
                let Some(row) = (m.row as usize)
                    .checked_sub(usize::from(render::FEED_TOP) + lead)
                    .map(|r| r + first)
                else {
                    return;
                };
                if app.panel.rows.get(row).is_some_and(PRow::selectable) {
                    app.panel.selected = row;
                    app.panel.focus = Focus::Panel;
                    let _ = app.panel_enter();
                }
            }
            _ => return,
        }
        app.dirty = true;
        return;
    }

    match m.kind {
        MouseEventKind::ScrollUp => app.scroll_by(-3),
        MouseEventKind::ScrollDown => app.scroll_by(3),
        MouseEventKind::Down(MouseButton::Left) => {
            let top = render::FEED_TOP;
            if m.row < top {
                return;
            }
            // A conversation shorter than the pane sits against the bottom, so
            // the blank rows above it belong to nothing and must not select
            // the first message.
            let Some(within) = ((m.row - top) as usize).checked_sub(app.feed_pad) else {
                return;
            };
            let line = app.scroll + within;
            let Some(row) = app.row_starts.iter().rposition(|&s| s <= line) else {
                return;
            };
            if row >= app.view.len() {
                return;
            }
            let same = row == app.selected;
            app.selected = row;
            app.follow = false;
            if same {
                app.toggle_expand();
            }
            app.dirty = true;
        }
        _ => {}
    }
}
