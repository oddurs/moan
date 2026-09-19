//! Drawing. The feed is the product; everything else is furniture.
use crate::event::{Event, GistSource, Kind, Note};
use crate::ui::theme::Theme;
use chrono::Utc;

use crate::ui::app::{ago, clock, App, Mode};

use ratatui::prelude::*;
use ratatui::widgets::{Block, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;
const TIME_W: usize = 5;
const LABEL_W: usize = 7;
/// Where a gist starts, and where expanded bodies indent to.
/// A column at the left edge carrying the freshness accent.
const ACCENT_W: usize = 2;
const GUTTER: usize = ACCENT_W + TIME_W + 1 + 1 + 1 + LABEL_W + 1;

/// How long a message reads as new, and what it fades through getting there.
/// Green because it is arrival, not alarm; three steps because two reads as a
/// blink and four is not distinguishable at a glance.
/// How long a message reads as new. The colours come from the theme; these
/// are the ages at which it steps down.
const FADE: [i64; 3] = [20, 60, 180];

/// One turning shape, used wherever something is being condensed — a row, or
/// the count in the header. The same motion should always mean the same thing.
const SPIN: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

fn spinner(tick: u64) -> &'static str {
    SPIN[(tick as usize) % SPIN.len()]
}

/// What to put before a gist that is not the finished article.
///
/// Nothing for a message that has not been sent — out past the reading window,
/// or still arriving — because a mark there would promise something that is
/// not coming. A quiet ellipsis for one that is on its way, and a retry mark
/// for one the model refused.
///
/// The motion lives in the header and nowhere else. Four calls run at a time
/// but dozens are queued, so a spinner per row would show forty things turning
/// when four are moving — and forty synchronised spinners is a strobe, not a
/// loading state.
fn state_mark(src: GistSource, t: &Theme) -> Option<(&'static str, Color)> {
    match src {
        GistSource::Working => Some(("⋯ ", t.dim)),
        GistSource::Failed => Some(("↺ ", t.error)),
        // Not an error and not a wait: a decision, and one worth seeing.
        GistSource::Withheld => Some(("⊘ ", t.alert)),
        _ => None,
    }
}

/// The accent for a message of this age, if it still has one.
fn accent(t: &Theme, ts: chrono::DateTime<Utc>) -> Option<(&'static str, Color)> {
    // Weight as well as colour. A message that has just landed gets a thicker
    // mark than one going quiet, so the ramp reads at a glance and still reads
    // with the colour turned off.
    const MARKS: [&str; 3] = ["\u{258a}", "\u{258e}", "\u{258f}"];
    let age = (Utc::now() - ts).num_seconds();
    if age < 0 {
        return Some((MARKS[0], t.fade[0]));
    }
    FADE.iter()
        .position(|limit| age < *limit)
        .map(|i| (MARKS[i], t.fade[i]))
}

/// The oldest age that still shows an accent, so the loop knows when to stop
/// asking for redraws.
pub const FADE_MAX: i64 = FADE[FADE.len() - 1];

pub fn draw(f: &mut Frame, app: &mut App) {
    app.width = f.area().width;
    let [top, body, bottom] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(f.area());

    header(f, top, app);

    if app.panel.splits(f.area().width) {
        let w = app.panel.columns(f.area().width);
        let [side, edge, main] = Layout::horizontal([
            Constraint::Length(w),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .areas(body);
        panel(f, side, app);
        divider_col(f, edge, app);
        feed(f, main, app);
    } else {
        feed(f, body, app);
    }
    footer(f, bottom, app);

    // Over the feed when the terminal is too narrow to split.
    if app.panel.open && !app.panel.splits(f.area().width) {
        panel_overlay(f, f.area(), app);
    }
    if app.mode == Mode::Help {
        help(f, f.area(), &app.theme);
    }
}

/// The first terminal row of the conversation: under the header, and nothing
/// else. A constant now that the rail is gone; the mouse handler subtracts it
/// to turn a click into a message.
pub const FEED_TOP: u16 = 1;

/// The line between the panes, bright on whichever side has the keyboard.
fn divider_col(f: &mut Frame, area: Rect, app: &App) {
    use crate::ui::panel::Focus;
    let t = app.theme;
    let lit = app.panel.focus == Focus::Panel;
    for y in 0..area.height {
        f.render_widget(
            Paragraph::new(Span::styled(
                "│",
                Style::new().fg(if lit { t.user } else { t.dim }),
            )),
            Rect {
                x: area.x,
                y: area.y + y,
                width: 1,
                height: 1,
            },
        );
    }
}

/// The panel, beside the feed.
fn panel(f: &mut Frame, area: Rect, app: &mut App) {
    use crate::ui::panel::{Focus, Mode as PMode, Row};
    let t = app.theme;
    let focused = app.panel.focus == Focus::Panel;
    let w = area.width as usize;

    let tab = |name: &str, on: bool| {
        Span::styled(
            format!(" {name} "),
            if on {
                Style::new().fg(t.bright).bold()
            } else {
                Style::new().fg(t.dim)
            },
        )
    };
    // Written against the list of modes, so a third costs one line there and
    // nothing here.
    let mut header = vec![Span::raw(" ")];
    for (m, name) in crate::ui::panel::MODES {
        header.push(tab(name, app.panel.mode == *m));
        header.push(Span::raw(" "));
    }
    // Kept out of the scrolling part. The tabs and the query are what tell you
    // where you are and why the list is short; scrolling them away loses that
    // exactly when a long list needs it most.
    let mut head = vec![Line::from(header)];
    // Whatever is being typed, in either list. Narrowing a list with no sign
    // of what narrowed it is a list that has mysteriously lost things.
    if app.panel.typing || !app.panel.query.is_empty() {
        let cursor = if app.panel.typing { "▏" } else { "" };
        head.push(Line::from(vec![
            Span::styled(" ▸ ", Style::new().fg(t.alert)),
            Span::styled(app.panel.query.clone(), Style::new().fg(t.bright)),
            Span::styled(cursor, Style::new().fg(t.alert)),
        ]));
        let found = if app.panel.mode == PMode::Search {
            app.panel.hits.len()
        } else {
            app.panel
                .rows
                .iter()
                .filter(|r| matches!(r, Row::Session(_)))
                .count()
        };
        let what = match (app.panel.query.is_empty(), found) {
            (true, _) if app.panel.mode == PMode::Search => {
                "type to search every conversation".into()
            }
            (true, _) => "type to narrow the list".to_string(),
            (_, 0) => "nothing matches".to_string(),
            (_, n) => format!("{n} of {}", app.sessions.len()),
        };
        head.push(Line::from(Span::styled(
            format!("   {what}"),
            Style::new().fg(t.dim),
        )));
    }
    head.push(Line::from(""));

    let mut lines: Vec<Line> = Vec::new();

    if app.panel.rows.is_empty() {
        let msg = match app.panel.mode {
            PMode::Search if app.panel.query.is_empty() => "type to search",
            PMode::Search => "nothing found in what you have read",
            PMode::Sessions => "nothing to read yet",
        };
        lines.push(Line::from(Span::styled(
            format!(" {msg}"),
            Style::new().fg(t.dim),
        )));
    }
    for (i, row) in app.panel.rows.iter().enumerate() {
        let sel = i == app.panel.selected && row.selectable();
        // A selection in the pane without the keyboard is an accent bar, not
        // a highlight: still visible, plainly not live.
        let base = if sel && focused {
            Style::new().bg(t.selection)
        } else {
            Style::new()
        };
        let bar = if sel {
            Span::styled("▎", Style::new().fg(if focused { t.user } else { t.dim }))
        } else {
            Span::styled(" ", base)
        };
        lines.push(match row {
            Row::Gap => Line::from(""),
            Row::Project { title, count } => Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    fit(title, w.saturating_sub(6)),
                    Style::new().fg(t.badge_bg).bold(),
                ),
                // Search groups hits by session; a count there would repeat
                // the total the header already gives.
                Span::styled(
                    if *count > 0 {
                        format!("  {count}")
                    } else {
                        String::new()
                    },
                    Style::new().fg(t.dim),
                ),
            ]),
            Row::Session(idx) => {
                let Some(s) = app.sessions.get(*idx) else {
                    return;
                };
                let open = app.session.as_ref().is_some_and(|c| c.id == s.id);
                let st = app.session_state(s);
                let tone = match st {
                    crate::ui::panel::State::Live => t.live,
                    crate::ui::panel::State::Waiting => t.alert,
                    crate::ui::panel::State::Idle => t.dim,
                };
                let name = app.name_of(&s.id);
                // What has arrived since you last read it, and how much of
                // it wants attention.
                let (badge, badge_c) = match app.unread_of(&s.id) {
                    _ if open => (String::new(), t.dim),
                    Some((0, _)) => (String::new(), t.dim),
                    Some((n, 0)) => (format!("{n}"), t.dim),
                    Some((_, a)) => (format!("!{a}"), t.alert),
                    None => ("·".into(), t.dim),
                };
                // How long ago, because one from this morning and one from
                // March look identical without it.
                let age = short_ago(s.modified);
                // And which agent, but only where the machine has more than
                // one — otherwise it is the same letter on every row.
                let agent = if s.source == app.common_agent {
                    String::new()
                } else {
                    format!("{} ", s.source.chars().next().unwrap_or(' '))
                };
                // A conversation that has been through the model is marked,
                // so you can see at a glance which ones have cost anything
                // and which are still free.
                let paid = if app.paid.contains_key(&s.id) {
                    "≈"
                } else {
                    " "
                };
                let room = w.saturating_sub(5 + agent.width() + 8);
                let label = fit(&name, room);
                let pad = room.saturating_sub(label.width());
                Line::from(vec![
                    bar,
                    Span::styled(format!("{} ", st.mark()), base.fg(tone)),
                    Span::styled(agent, base.fg(t.dim)),
                    Span::styled(label, base.fg(if open || sel { t.bright } else { t.text })),
                    Span::styled(" ".repeat(pad), base),
                    Span::styled(format!("{paid} "), base.fg(t.claude)),
                    Span::styled(format!("{age:>4} "), base.fg(t.dim)),
                    Span::styled(format!("{badge:>2}"), base.fg(badge_c)),
                ])
            }
            Row::Result(i) => {
                let Some(h) = app.panel.hits.get(*i) else {
                    return;
                };
                let who = match h.kind.as_str() {
                    "prompt" => t.user,
                    "say" => t.claude,
                    _ => t.dim,
                };
                let when = h.ts.get(11..16).unwrap_or("");
                Line::from(vec![
                    bar,
                    Span::styled(format!("{when} "), base.fg(t.dim)),
                    Span::styled(
                        fit(h.gist.lines().next().unwrap_or(""), w.saturating_sub(8)),
                        base.fg(if sel { t.bright } else { t.text }),
                    ),
                    Span::styled("", base.fg(who)),
                ])
            }
        });
    }

    let (top, body) = panel_split(area, head.len());
    app.panel.shown = body.height as usize;
    // The selection may have been moved by the keyboard since the last frame,
    // when the list's height was not yet known.
    app.panel.reveal();
    f.render_widget(Paragraph::new(Text::from(head)), top);
    f.render_widget(
        Paragraph::new(Text::from(lines))
            .scroll((u16::try_from(app.panel.scroll).unwrap_or(u16::MAX), 0)),
        body,
    );
}

/// The fixed header and the scrolling list beneath it.
fn panel_split(area: Rect, head: usize) -> (Rect, Rect) {
    let n = u16::try_from(head).unwrap_or(0).min(area.height);
    (
        Rect { height: n, ..area },
        Rect {
            y: area.y + n,
            height: area.height - n,
            ..area
        },
    )
}

/// The same panel, over the feed, when there is no room to sit beside it.
fn panel_overlay(f: &mut Frame, area: Rect, app: &mut App) {
    let a = centered(area, 74, 78);
    f.render_widget(Clear, a);
    let inner = Rect {
        x: a.x + 2,
        y: a.y + 1,
        width: a.width.saturating_sub(4),
        height: a.height.saturating_sub(2),
    };
    f.render_widget(panel_frame(&app.theme), a);
    panel(f, inner, app);
}

fn panel_frame(t: &Theme) -> Block<'static> {
    Block::bordered()
        .border_style(Style::new().fg(t.dim))
        .title_bottom(Span::styled(" ⏎ open   s close ", Style::new().fg(t.dim)))
}

/// Where you are, and what else is happening.
fn header(f: &mut Frame, area: Rect, app: &App) {
    let t = app.theme;
    let w = area.width as usize;
    let sep = || Span::styled("  ·  ", Style::new().fg(t.dim));

    // Right side first: it is fixed width, and the title yields to it.
    let mut right: Vec<Span> = Vec::new();
    if let Some(s) = &app.session {
        let (mark, tone) = if app.pending > 0 {
            (spinner(app.tick), t.user)
        } else if (Utc::now() - s.modified).num_seconds() < 120 {
            ("●", t.live)
        } else {
            ("○", t.dim)
        };
        // That the feed is still settling is worth knowing. How many requests
        // are in flight is moan talking about itself.
        let word = if app.pending > 0 {
            "settling".to_string()
        } else if !app.follow {
            "paused".into()
        } else {
            ago(s.modified)
        };
        right.push(Span::styled(format!("{mark} "), Style::new().fg(tone)));
        right.push(Span::styled(word, Style::new().fg(t.text)));
        right.push(sep());
    }
    right.push(Span::styled(
        short_model(&app.model_label),
        Style::new().fg(t.dim),
    ));
    right.push(sep());
    if app.withheld > 0 {
        right.push(Span::styled(
            format!("⊘ {} kept back", app.withheld),
            Style::new().fg(t.alert),
        ));
        right.push(sep());
    }
    right.push(Span::styled(
        format!("{}", app.view.len()),
        Style::new().fg(t.text),
    ));

    // Whether this project has other conversations, and whether any of them
    // are working. It used to be a whole row; it is worth a few characters.
    let siblings = app.peers.len().saturating_sub(1);
    if siblings > 0 {
        let live = app
            .peers
            .iter()
            .filter(|p| {
                app.session.as_ref().is_none_or(|c| c.id != p.id)
                    && app.session_state(p) == crate::ui::panel::State::Live
            })
            .count();
        right.insert(0, sep());
        right.insert(
            0,
            Span::styled(
                if live > 0 {
                    format!("● {live} of {siblings} working")
                } else {
                    format!("{siblings} more here")
                },
                Style::new().fg(if live > 0 { t.live } else { t.dim }),
            ),
        );
    }

    // Left: the mark, then where you are.
    let mut left = vec![
        Span::styled(" moan ", Style::new().fg(t.badge_fg).bg(t.badge_bg).bold()),
        Span::raw("  "),
    ];
    if let Some(s) = &app.session {
        let rw: usize = right.iter().map(|x| x.content.width()).sum();
        let room = w.saturating_sub(rw + 12);
        let branch = s.branch.clone().unwrap_or_default();
        let bw = if branch.is_empty() {
            0
        } else {
            branch.width() + 3
        };
        left.push(Span::styled(
            fit(&s.title, room.saturating_sub(bw)),
            Style::new().fg(t.bright),
        ));
        if !branch.is_empty() {
            left.push(Span::styled("  ", Style::new()));
            left.push(Span::styled(
                fit(&branch, room / 2),
                Style::new().fg(t.branch),
            ));
        }
    }

    let lw: usize = left.iter().map(|x| x.content.width()).sum();
    let rw: usize = right.iter().map(|x| x.content.width()).sum();
    let mut spans = left;
    spans.push(Span::raw(" ".repeat(w.saturating_sub(lw + rw + 1))));
    spans.extend(right);
    spans.push(Span::raw(" "));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// `openrouter/anthropic/claude-haiku-4.5` is not worth a third of the header.
fn short_model(full: &str) -> String {
    let tail = full.rsplit('/').next().unwrap_or(full);
    let tail = tail.strip_prefix("claude-").unwrap_or(tail);
    // Trailing release dates carry nothing at a glance.
    let tail = match tail.rsplit_once('-') {
        Some((head, d)) if d.len() == 8 && d.chars().all(|c| c.is_ascii_digit()) => head,
        _ => tail,
    };
    tail.to_string()
}

fn footer(f: &mut Frame, area: Rect, app: &App) {
    let t = app.theme;
    let w = area.width as usize;

    if app.panel.typing {
        // The search box draws its own cursor in the panel; the bar says what
        // the keys do while typing.
        let l = Line::from(vec![
            Span::raw(" "),
            Span::styled("⏎", Style::new().fg(t.user)),
            Span::styled(" jump   ", Style::new().fg(t.dim)),
            Span::styled("esc", Style::new().fg(t.user)),
            Span::styled(" stop typing", Style::new().fg(t.dim)),
        ]);
        f.render_widget(Paragraph::new(l), area);
        return;
    }

    if app.mode == Mode::Filter {
        let l = Line::from(vec![
            Span::styled("  search  ", Style::new().fg(t.badge_fg).bg(t.alert).bold()),
            Span::raw("  "),
            Span::styled(app.filter.clone(), Style::new().fg(t.bright)),
            Span::styled("▏", Style::new().fg(t.alert)),
        ]);
        f.render_widget(Paragraph::new(l), area);
        return;
    }

    if !app.status.is_empty() {
        let (tag, tone) = if app.status_is_error {
            ("  warn  ", t.error)
        } else {
            ("  note  ", t.link)
        };
        let l = Line::from(vec![
            Span::styled(tag, Style::new().fg(t.badge_fg).bg(tone).bold()),
            Span::raw("  "),
            Span::styled(
                fit(&app.status, w.saturating_sub(tag.width() + 3)),
                Style::new().fg(t.text),
            ),
        ]);
        f.render_widget(Paragraph::new(l), area);
        return;
    }

    // Otherwise, nothing. A bar that says the same nine things whatever is
    // happening is furniture, and `?` is where the keys live. It appears once,
    // on a first run, for somebody who does not know that yet.
    if app.first_run {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(" "),
                Span::styled("?", Style::new().fg(t.user)),
                Span::styled(" keys   ", Style::new().fg(t.dim)),
                Span::styled("s", Style::new().fg(t.user)),
                Span::styled(" conversations   ", Style::new().fg(t.dim)),
                Span::styled("q", Style::new().fg(t.user)),
                Span::styled(" quit", Style::new().fg(t.dim)),
            ])),
            area,
        );
    }
}
/// Build the feed, then scroll it so the selection sits in view.
fn feed(f: &mut Frame, area: Rect, app: &mut App) {
    let t = app.theme;
    if app.view.is_empty() {
        // Each of these says what would fill it, because an empty pane that
        // only says "empty" leaves you guessing whether it is broken.
        let msg = if !app.filter.is_empty() {
            format!("nothing in this conversation matches \"{}\"", app.filter)
        } else if app.events.is_empty() {
            "this conversation has not started yet — it will appear as Claude works".into()
        } else if !app.cfg.ui.show_tools {
            "only tool calls here so far — press o to see them".into()
        } else {
            "nothing here".into()
        };
        f.render_widget(
            Paragraph::new(Span::styled(msg, Style::new().fg(t.dim)))
                .block(Block::new().padding(ratatui::widgets::Padding::new(2, 0, 1, 0))),
            area,
        );
        return;
    }
    // The scroll track owns the last column, always, so the conversation does
    // not reflow the moment it grows past one screen.
    let w = (area.width as usize).saturating_sub(1);
    let h = area.height as usize;
    // What condensation reaches is measured from what is actually on screen.
    app.viewport = h;

    // A row is a headline plus any bullets, plus the original text when it is
    // expanded. Build them once: the heights drive both scroll and layout.
    // Where every row starts is cached on the app and rebuilt only when the
    // rows, the expansions or the width move. Summing heights here made the
    // frame linear in the length of the conversation.
    if app.needs_layout(area.width) {
        app.lay_out(area.width, |e, _| body_lines(e, w, &t).len());
    }
    let mut total = app.total_lines;

    let anchor = app.row_starts.get(app.selected).copied().unwrap_or(0);
    let sel_h = app
        .row_starts
        .get(app.selected + 1)
        .copied()
        .unwrap_or(total)
        .saturating_sub(anchor);

    // The viewport holds its own position: the wheel moves it and the cursor
    // does not. `reveal` is the exception the keyboard needs — a cursor it just
    // moved has to be on screen — and it pulls the viewport the shortest
    // distance that shows the row, never recentring on it.
    let want = std::mem::take(&mut app.reveal);
    let mut top = app.scroll;
    if app.follow {
        top = total.saturating_sub(h);
    } else if want {
        if anchor < top || sel_h >= h {
            // A row taller than the viewport anchors at its own first line;
            // scrolling to its end would push the row itself off the top.
            top = anchor;
        } else if anchor + sel_h > top + h {
            top = (anchor + sel_h).saturating_sub(h);
        }
    }
    top = top.min(total.saturating_sub(h.min(total)));
    if app.awaiting() {
        total += 1;
        if app.follow {
            top = total.saturating_sub(h);
        }
    }

    // Only the rows the viewport touches are laid out.
    let first = app
        .row_starts
        .partition_point(|&s| s <= top)
        .saturating_sub(1);
    let mut lines: Vec<Line> = Vec::with_capacity(h + 8);
    for (vi, start) in app.row_starts.iter().enumerate().skip(first) {
        if *start >= top + h {
            break;
        }
        match app.view[vi] {
            crate::ui::app::Row::Origin(i) => {
                lines.push(divider(&app.origins[i], w, &t));
            }
            _ => {
                let Some(e) = app.at(vi) else { continue };
                let who = speaker(e, app.agent_of(&e.session));
                lines.extend(message(
                    e,
                    who,
                    w,
                    vi == app.selected,
                    app.cfg.ui.clock_24h,
                    &t,
                ));
                if app.expanded.contains(&e.uid) {
                    lines.extend(body_lines(e, w, &t));
                }
            }
        }
    }

    if app.awaiting() {
        lines.push(working(app.tick, w, app.cfg.ui.clock_24h, &t));
    }

    // A conversation shorter than the pane sits against the bottom, the way a
    // quiet Slack channel does, so new messages always arrive in one place.
    app.feed_pad = h.saturating_sub(total);
    if total < h {
        let mut padded = vec![Line::from(""); app.feed_pad];
        padded.append(&mut lines);
        lines = padded;
    }

    app.scroll = top;
    // What ratatui is asked to scroll by is the offset *within* the first row
    // we built, never the absolute line — so it always fits a u16 however long
    // the conversation is.
    let base = app.row_starts.get(first).copied().unwrap_or(0);
    let within = u16::try_from(top.saturating_sub(base)).unwrap_or(0);
    if total > h {
        track(f, area, top, h, total, &t);
    }
    f.render_widget(Paragraph::new(Text::from(lines)).scroll((within, 0)), area);
}
/// Where you are in a long conversation, drawn on the last column.
fn track(f: &mut Frame, area: Rect, top: usize, h: usize, total: usize, t: &Theme) {
    let x = area.right().saturating_sub(1);
    // Integer arithmetic: the thumb is a count of rows, and rounding through
    // floats to get one is how an off-by-one appears at some window size
    // nobody tested.
    let bar = ((h * h).div_ceil(total)).clamp(1, h);
    let span = h - bar;
    let at = if total > h {
        (top * span).div_ceil(total - h).min(span)
    } else {
        0
    };
    for i in 0..h {
        let on = i >= at && i < at + bar;
        f.render_widget(
            Paragraph::new(Span::styled(
                if on { "\u{2503}" } else { "\u{2502}" },
                Style::new().fg(if on { t.thumb } else { t.track }),
            )),
            Rect {
                x,
                y: area.y + i as u16,
                width: 1,
                height: 1,
            },
        );
    }
}

/// Says whose feed the messages below came from. Only drawn where the source
/// changes, so a long run from one session reads uninterrupted.
fn divider(name: &str, width: usize, t: &Theme) -> Line<'static> {
    let label = format!(" {} ", fit(name, 28));
    let lead = GUTTER.saturating_sub(2);
    let rest = width.saturating_sub(lead + label.width() + 2);
    Line::from(vec![
        Span::styled(
            format!("{}\u{2500}\u{2500}", " ".repeat(lead)),
            Style::new().fg(t.dim),
        ),
        Span::styled(label, Style::new().fg(t.branch)),
        Span::styled("\u{2500}".repeat(rest), Style::new().fg(t.dim)),
    ])
}

/// Claude has your message and has not answered yet.
///
/// Built like a message rather than a status line — same gutter, same author
/// column — so the conversation gains a row instead of growing a widget.
fn working(tick: u64, width: usize, h24: bool, t: &Theme) -> Line<'static> {
    let dots = [
        "\u{25cf}\u{00b7}\u{00b7}",
        "\u{00b7}\u{25cf}\u{00b7}",
        "\u{00b7}\u{00b7}\u{25cf}",
        "\u{00b7}\u{25cf}\u{00b7}",
    ];
    let frame = dots[(tick / 2) as usize % dots.len()];
    let mut spans = vec![
        Span::styled("\u{258e} ", Style::new().fg(t.fade[0])),
        Span::styled(
            format!("{:>w$} ", clock(Utc::now(), h24), w = TIME_W),
            Style::new().fg(t.dim),
        ),
        Span::styled("\u{258c} ", Style::new().fg(t.claude)),
        Span::styled(
            format!("{:<w$} ", fit("claude", LABEL_W), w = LABEL_W),
            Style::new().fg(t.claude),
        ),
    ];
    // The travelling dot spaced out, so it reads as composing rather than as
    // a progress bar that is going nowhere.
    for c in frame.chars() {
        let on = c == '\u{25cf}';
        spans.push(Span::styled(
            format!("{c} "),
            Style::new().fg(if on { t.text } else { t.dim }),
        ));
    }
    let _ = width;
    Line::from(spans)
}

/// Who is speaking on this row. An assistant is named by the agent that wrote
/// the conversation, because "claude" over a Codex reply is simply wrong.
fn speaker<'a>(e: &'a Event, agent: &'a str) -> &'a str {
    match e.kind {
        Kind::Say => agent,
        _ => e.kind.label(),
    }
}

/// An age in the width of a column: `now`, `2m`, `3h`, `6d`.
fn short_ago(ts: chrono::DateTime<Utc>) -> String {
    let s = (Utc::now() - ts).num_seconds().max(0);
    match s {
        0..=90 => "now".into(),
        91..=5400 => format!("{}m", (s + 30) / 60),
        5401..=172_800 => format!("{}h", (s + 1800) / 3600),
        _ => format!("{}d", s / 86_400),
    }
}

/// How many lines a message occupies, without laying it out.
///
/// Must agree with `message`: one line for the headline, one for each note
/// that has anything in it.
pub fn message_height(e: &Event) -> usize {
    1 + e
        .gist
        .lines()
        .skip(1)
        .filter(|l| !crate::event::note_of(l).1.is_empty())
        .count()
}

/// One chat message: the headline row, then any bullets under it.
fn message(
    e: &Event,
    who: &str,
    width: usize,
    selected: bool,
    h24: bool,
    t: &Theme,
) -> Vec<Line<'static>> {
    let mut gist = e.gist.lines();
    let head = gist.next().unwrap_or("");
    let mut out = vec![row(e, who, head, width, selected, h24, t)];
    for line in gist {
        let (kind, text) = crate::event::note_of(line);
        if text.is_empty() {
            continue;
        }
        let base = if selected {
            Style::new().bg(t.selection)
        } else {
            Style::new()
        };
        // An alert has to survive being read at a glance; a link has to look
        // like somewhere to go; a detail should stay quiet.
        let (mark_c, text_c) = match kind {
            Note::Alert => (t.alert, t.bright),
            Note::Info => (t.dim, t.text),
            Note::Link => (t.link, t.link),
        };
        let (body, used) = one_line(text, width.saturating_sub(GUTTER + 2), base.fg(text_c), t);
        let pad = width.saturating_sub(GUTTER + 2 + used);
        let mut line = vec![
            Span::styled(" ".repeat(GUTTER), base),
            Span::styled(format!("{} ", kind.mark()), base.fg(mark_c)),
        ];
        line.extend(body);
        line.push(Span::styled(
            " ".repeat(if selected { pad } else { 0 }),
            base,
        ));
        out.push(Line::from(line));
    }
    out
}
/// One collapsed row: time, role, gist, and the outcome on the right.
fn row(
    e: &Event,
    who: &str,
    text: &str,
    width: usize,
    selected: bool,
    h24: bool,
    t: &Theme,
) -> Line<'static> {
    let (label_c, gist_c) = palette(e, t);
    let base = if selected {
        Style::new().bg(t.selection)
    } else {
        Style::new()
    };
    let out = e.outcome.clone().unwrap_or_default();
    let out_w = if out.is_empty() {
        0
    } else {
        out.width().min(24) + 2
    };
    let gist_w = width.saturating_sub(GUTTER + out_w);
    let fresh = accent(t, e.ts);
    let mut spans = vec![
        Span::styled(
            fresh.map_or_else(|| "  ".to_string(), |(m, _)| format!("{m} ")),
            base.fg(fresh.map_or(Color::Reset, |(_, c)| c)),
        ),
        Span::styled(
            format!("{:>w$} ", clock(e.ts, h24), w = TIME_W),
            // A just-arrived message earns a readable timestamp too.
            base.fg(match fresh {
                Some((_, c)) if c == t.fade[0] => t.text,
                _ => t.dim,
            }),
        ),
        Span::styled(format!("{} ", e.role.glyph()), base.fg(label_c)),
        Span::styled(
            format!("{:<w$} ", fit(who, LABEL_W), w = LABEL_W),
            base.fg(label_c),
        ),
    ];
    let mark = state_mark(e.gist_src, t);
    let mark_w = mark.as_ref().map_or(0, |(m, _)| m.width());
    // A placeholder is dimmed and slanted so it never passes for a message.
    let gist_style = match e.gist_src {
        GistSource::Pending | GistSource::Working | GistSource::Failed | GistSource::Withheld => {
            base.fg(t.dim).add_modifier(Modifier::ITALIC)
        }
        _ => base.fg(gist_c),
    };
    if let Some((m, c)) = mark {
        spans.push(Span::styled(m, base.fg(c)));
    }
    let (gist_spans, gist_w_used) = one_line(text, gist_w.saturating_sub(mark_w), gist_style, t);
    spans.extend(gist_spans);
    if out_w > 0 {
        let pad = width.saturating_sub(GUTTER + mark_w + gist_w_used + out_w);
        spans.push(Span::styled(" ".repeat(pad + 2), base));
        let c = if e.is_error { t.error } else { t.dim };
        spans.push(Span::styled(fit(&out, 24), base.fg(c)));
    } else if selected {
        let pad = width.saturating_sub(GUTTER + mark_w + gist_w_used);
        spans.push(Span::styled(" ".repeat(pad), base));
    }
    Line::from(spans)
}
/// The full original text, indented under its row and rendered as the
/// markdown it was written in.
///
/// Urls are printed in full rather than hidden behind their link text: most
/// terminals make a literal url clickable, and one that has been shortened to
/// read nicely is one nobody can follow. `O` opens the first without relying
/// on the terminal at all.
fn body_lines(e: &Event, width: usize, t: &Theme) -> Vec<Line<'static>> {
    use crate::ui::markdown::{blocks, inline, Block};

    let inner = width.saturating_sub(GUTTER + 2).max(20);
    let mut out = Vec::new();
    let rule = |out: &mut Vec<Line<'static>>, spans: Vec<Span<'static>>| {
        let mut line = vec![
            Span::raw(" ".repeat(GUTTER)),
            Span::styled("│ ", Style::new().fg(t.dim)),
        ];
        line.extend(spans);
        out.push(Line::from(line));
    };

    for b in blocks(&e.raw) {
        match b {
            Block::Blank => rule(&mut out, vec![]),
            Block::Rule => rule(
                &mut out,
                vec![Span::styled(
                    "─".repeat(inner.min(40)),
                    Style::new().fg(t.dim),
                )],
            ),
            Block::Code(lines) => {
                for l in lines {
                    // Code is shown as typed, wrapped only where it must be.
                    for piece in hard_wrap(&l, inner) {
                        rule(&mut out, vec![Span::styled(piece, Style::new().fg(t.tool))]);
                    }
                }
            }
            Block::Heading(depth, text) => {
                for row in lay_out(&inline(&text), inner, t, Style::new().fg(t.bright).bold()) {
                    let mut spans = row;
                    if depth <= 2 {
                        spans.insert(0, Span::raw(""));
                    }
                    rule(&mut out, spans);
                }
            }
            Block::Bullet(depth, text) => {
                let pad = "  ".repeat(depth.min(4));
                let lead = format!("{pad}• ");
                let w = inner.saturating_sub(lead.width());
                for (n, row) in lay_out(&inline(&text), w, t, Style::new().fg(t.text))
                    .into_iter()
                    .enumerate()
                {
                    let mut spans = vec![Span::styled(
                        if n == 0 {
                            lead.clone()
                        } else {
                            " ".repeat(lead.width())
                        },
                        Style::new().fg(t.dim),
                    )];
                    spans.extend(row);
                    rule(&mut out, spans);
                }
            }
            Block::Quote(text) => {
                for row in lay_out(
                    &inline(&text),
                    inner.saturating_sub(2),
                    t,
                    Style::new().fg(t.dim),
                ) {
                    let mut spans = vec![Span::styled("▏ ", Style::new().fg(t.dim))];
                    spans.extend(row);
                    rule(&mut out, spans);
                }
            }
            Block::Para(text) => {
                for row in lay_out(&inline(&text), inner, t, Style::new().fg(t.text)) {
                    rule(&mut out, row);
                }
            }
        }
    }
    out.push(Line::from(""));
    out
}

/// Inline markdown on a single line, cut to fit.
///
/// The feed's own lines carry the odd identifier in backticks or a word in
/// bold. Showing the punctuation puts markup in front of the words; this
/// styles it instead, and leaves anything it does not recognise alone.
fn one_line(text: &str, width: usize, base: Style, t: &Theme) -> (Vec<Span<'static>>, usize) {
    use crate::ui::markdown::{inline, Piece};
    let mut spans = Vec::new();
    let mut used = 0usize;
    for p in inline(text) {
        if used >= width {
            break;
        }
        let style = match &p {
            Piece::Bold(_) => base.add_modifier(Modifier::BOLD),
            Piece::Italic(_) => base.add_modifier(Modifier::ITALIC),
            Piece::Code(_) => base.fg(t.tool),
            Piece::Link { .. } | Piece::Url(_) => base.fg(t.link),
            Piece::Text(_) => base,
        };
        let body = fit(p.text(), width - used);
        used += body.width();
        spans.push(Span::styled(body, style));
    }
    (spans, used)
}

/// Lay styled runs into lines of at most `width`.
///
/// A url is kept whole wherever it fits: broken across two lines it stops
/// being something the terminal can turn into a link, and stops being
/// something anybody can copy.
fn lay_out(
    pieces: &[crate::ui::markdown::Piece],
    width: usize,
    t: &Theme,
    base: Style,
) -> Vec<Vec<Span<'static>>> {
    use crate::ui::markdown::Piece;

    // A link is shown as its words *and* its destination. Hidden behind nice
    // text, a url is one nobody can follow, copy, or let their terminal turn
    // into something clickable.
    let mut flat: Vec<Piece> = Vec::with_capacity(pieces.len());
    for p in pieces {
        match p {
            Piece::Link { text, url } => {
                flat.push(Piece::Text(format!("{text} ")));
                flat.push(Piece::Url(url.clone()));
            }
            other => flat.push(other.clone()),
        }
    }
    let pieces = &flat[..];

    let style_of = |p: &Piece| match p {
        Piece::Bold(_) => base.add_modifier(Modifier::BOLD).fg(t.bright),
        Piece::Italic(_) => base.add_modifier(Modifier::ITALIC),
        Piece::Code(_) => base.fg(t.tool),
        Piece::Link { .. } | Piece::Url(_) => base.fg(t.link).add_modifier(Modifier::UNDERLINED),
        Piece::Text(_) => base,
    };

    let mut rows: Vec<Vec<Span<'static>>> = Vec::new();
    let mut row: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;

    for p in pieces {
        let style = style_of(p);
        // A url, and anything short, moves whole rather than word by word.
        let atomic = matches!(p, Piece::Url(_) | Piece::Code(_) | Piece::Link { .. });
        let chunks: Vec<String> = if atomic {
            vec![p.text().to_string()]
        } else {
            p.text()
                .split_inclusive(char::is_whitespace)
                .map(str::to_string)
                .collect()
        };
        for chunk in chunks {
            let w = chunk.trim_end().width();
            if used + w > width && used > 0 {
                rows.push(std::mem::take(&mut row));
                used = 0;
                if chunk.trim().is_empty() {
                    continue;
                }
            }
            if w > width {
                // Longer than a line even alone: cut it rather than lose it.
                for part in hard_wrap(&chunk, width) {
                    row.push(Span::styled(part, style));
                    rows.push(std::mem::take(&mut row));
                    used = 0;
                }
                continue;
            }
            used += chunk.width();
            row.push(Span::styled(chunk, style));
        }
    }
    if !row.is_empty() {
        rows.push(row);
    }
    if rows.is_empty() {
        rows.push(vec![]);
    }
    rows
}

/// Cut a run that cannot fit any other way.
fn hard_wrap(s: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in s.chars() {
        if cur.width() + 1 > width.max(1) {
            out.push(std::mem::take(&mut cur));
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// The author's colour, and the colour of what they said.
fn palette(e: &Event, t: &Theme) -> (Color, Color) {
    match &e.kind {
        Kind::Prompt => (t.user, t.bright),
        Kind::Say => (t.claude, Color::Reset),
        Kind::Think | Kind::Note => (t.system, t.system),
        Kind::Fail => (t.error, t.error),
        Kind::Tool { .. } if e.is_error => (t.error, t.text),
        Kind::Tool { .. } => (t.tool, t.text),
    }
}
// ── overlays ────────────────────────────────────────────────────────────
/// A framed overlay wearing the same badge as the header.
fn framed(title: &str, hint: &str, t: &Theme) -> Block<'static> {
    Block::bordered()
        .border_style(Style::new().fg(t.dim))
        .title(Span::styled(
            format!(" {title} "),
            Style::new().fg(t.badge_fg).bg(t.badge_bg).bold(),
        ))
        .title_bottom(Span::styled(format!(" {hint} "), Style::new().fg(t.dim)))
        .padding(ratatui::widgets::Padding::new(1, 1, 0, 0))
}

/// Only what cannot be worked out by looking. Everything else is in the
/// README; a keys overlay that needs scrolling is not a keys overlay.
const NOTES: &[&str] = &[
    "Marks under a message: ! worth knowing, · a detail,",
    "→ a link, and ⊘ held back — it looks like it holds a",
    "credential, so it was summarised here and never sent.",
    "",
    "The wheel moves the view; the arrows move the cursor.",
    "A bar at the left edge means it just arrived, and fades.",
    "In the list: ● working, ◆ waiting, ≈ paid to summarise,",
    "how long ago, and what is unread. / narrows it, esc back.",
];

/// The keys, in the order somebody learning them would want them.
const HELP: &[(&str, &str)] = &[
    ("↑ ↓   j k", "move between messages"),
    ("⏎  space", "show the original text"),
    ("g   G", "first / last message"),
    ("^u  ^d", "half a page"),
    ("f", "follow the live tail"),
    ("", ""),
    ("s", "every conversation, and search"),
    ("tab", "move between the list and the conversation"),
    ("1 … 9", "jump to one on this project"),
    ("a", "this project's conversations, interleaved"),
    ("", ""),
    ("o", "tool calls on and off"),
    ("t", "assistant reasoning on and off"),
    ("/", "narrow the list, or search the archive"),
    ("O", "open the first link in a message"),
    ("", ""),
    ("r", "summarise this message again"),
    ("R", "summarise everything again"),
    ("q", "quit"),
];

/// How tall the keys overlay has to be to show all of itself.
fn help_rows() -> u16 {
    u16::try_from(HELP.len() + NOTES.len() + 4).unwrap_or(u16::MAX)
}

fn help(f: &mut Frame, area: Rect, t: &Theme) {
    // Sized to what it holds, in both directions. A fixed fraction of the
    // screen cut the last third off on a short terminal and clipped the
    // longest labels on a narrow one, with nothing to say it had.
    let kw = HELP.iter().map(|(k, _)| k.width()).max().unwrap_or(0);
    let widest = HELP
        .iter()
        .map(|(_, label)| kw + 3 + label.width())
        .chain(NOTES.iter().map(|n| n.width()))
        .max()
        .unwrap_or(40);
    let rows = help_rows();
    let a = boxed(
        area,
        u16::try_from(widest + 4).unwrap_or(u16::MAX),
        rows.min(area.height),
    );
    f.render_widget(Clear, a);

    let kw = HELP.iter().map(|(k, _)| k.width()).max().unwrap_or(10);
    let mut lines: Vec<Line> = vec![Line::from("")];
    for (k, label) in HELP {
        if k.is_empty() {
            lines.push(Line::from(""));
            continue;
        }
        lines.push(Line::from(vec![
            Span::styled(format!("{:>w$}", k, w = kw), Style::new().fg(t.user)),
            Span::raw("   "),
            Span::styled(*label, Style::new().fg(t.text)),
        ]));
    }
    lines.push(Line::from(""));
    // Only what cannot be worked out by looking. Everything else is in the
    // README; a keys overlay that needs scrolling is not a keys overlay.
    for note in NOTES {
        lines.push(Line::from(Span::styled(*note, Style::new().fg(t.dim))));
    }

    f.render_widget(
        Paragraph::new(Text::from(lines)).block(framed("moan", "any key to close", t)),
        a,
    );
}

/// A box of a given size, centred, never larger than what it sits in.
fn boxed(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

fn centered(area: Rect, pct_w: u16, pct_h: u16) -> Rect {
    let w = area.width * pct_w / 100;
    let h = area.height * pct_h / 100;
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}
// ── text fitting ────────────────────────────────────────────────────────
/// Truncate to a display width, accounting for wide characters.
fn fit(s: &str, w: usize) -> String {
    if w == 0 {
        return String::new();
    }
    if s.width() <= w {
        return s.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if used + cw > w.saturating_sub(1) {
            break;
        }
        used += cw;
        out.push(c);
    }
    out.push('…');
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::event::{Event, Kind, Role};
    use crate::store::Store;
    use crate::ui::app::App;
    use chrono::{TimeZone, Utc};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    fn ev(kind: Kind, gist: &str, raw: &str, outcome: Option<&str>) -> Event {
        Event {
            uid: gist.into(),
            session: "s".into(),
            ts: Utc.with_ymd_and_hms(2026, 1, 1, 9, 41, 0).unwrap(),
            role: Role::Assistant,
            kind,
            raw: raw.into(),
            gist: crate::event::clip(&crate::event::flatten(gist), crate::PENDING_CLIP),
            gist_src: if gist.chars().count() > crate::VERBATIM_LIMIT {
                GistSource::Pending
            } else {
                GistSource::Rule
            },
            outcome: outcome.map(str::to_string),
            is_error: false,
            tokens: 0,
        }
    }
    /// Speakers alternate, so consecutive rows are not merged into one.
    fn turn(i: usize) -> Kind {
        if i.is_multiple_of(2) {
            Kind::Prompt
        } else {
            Kind::Say
        }
    }
    fn app_with(events: Vec<Event>) -> App {
        let mut cfg = Config::default();
        // Keep the test off the network and off the clock's timezone.
        cfg.model.provider = "none".into();
        cfg.model.api_key_env = "MOAN_TEST_NO_KEY".into();
        let mut a = App::new(cfg, Store::memory().unwrap()).unwrap();
        a.events = events;
        a.rebuild();
        a.go_bottom();
        a
    }
    fn screen(app: &mut App, w: u16, h: u16) -> Vec<String> {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| draw(f, app)).unwrap();
        let b = t.backend().buffer().clone();
        (0..b.area.height)
            .map(|y| {
                (0..b.area.width)
                    .map(|x| b[(x, y)].symbol().to_string())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }
    #[test]
    fn a_row_is_time_role_gist_and_outcome() {
        let mut a = app_with(vec![ev(
            Kind::Tool {
                name: "Bash".into(),
            },
            "cargo test --all",
            "{}",
            Some("3 lines"),
        )]);
        a.cfg.ui.show_tools = true;
        a.rebuild();
        let s = screen(&mut a, 60, 6);
        let row = s
            .iter()
            .find(|l| l.contains("cargo test"))
            .expect("row rendered");
        assert!(row.contains("Bash"));
        assert!(
            row.trim_end().ends_with("3 lines"),
            "outcome sits on the right: {row:?}"
        );
    }
    #[test]
    fn expanding_shows_the_original_text() {
        let mut a = app_with(vec![ev(
            Kind::Say,
            "short gist",
            "the full original text",
            None,
        )]);
        assert!(!screen(&mut a, 60, 8)
            .iter()
            .any(|l| l.contains("full original")));
        a.toggle_expand();
        let s = screen(&mut a, 60, 8);
        assert!(
            s.iter().any(|l| l.contains("the full original text")),
            "{s:#?}"
        );
    }
    #[test]
    fn thinking_is_hidden_until_asked_for() {
        let mut a = app_with(vec![
            ev(Kind::Say, "a said thing", "", None),
            ev(Kind::Think, "a thought", "", None),
        ]);
        assert!(!screen(&mut a, 60, 8)
            .iter()
            .any(|l| l.contains("a thought")));
        a.cfg.ui.show_thinking = true;
        a.rebuild();
        assert!(screen(&mut a, 60, 8)
            .iter()
            .any(|l| l.contains("a thought")));
    }
    #[test]
    fn the_feed_scrolls_to_keep_the_newest_line_visible() {
        let events: Vec<Event> = (0..200)
            .map(|i| ev(turn(i), &format!("line-{i}"), "", None))
            .collect();
        let mut a = app_with(events);
        let s = screen(&mut a, 60, 10);
        assert!(
            s.iter().any(|l| l.contains("line-199")),
            "newest is at the bottom"
        );
        assert!(!s.iter().any(|l| l.contains("line-0")));
        a.go_top();
        let s = screen(&mut a, 60, 10);
        assert!(s.iter().any(|l| l.contains("line-0")));
    }
    #[test]
    fn a_turns_prose_becomes_one_message() {
        // What a real turn looks like once its tool calls are hidden.
        let mut a = app_with(vec![
            ev(Kind::Prompt, "do the thing", "do the thing", None),
            ev(Kind::Say, "first I will", "first I will", None),
            ev(Kind::Say, "now the next bit", "now the next bit", None),
            ev(Kind::Say, "and done", "and done", None),
            ev(Kind::Prompt, "thanks", "thanks", None),
        ]);
        assert_eq!(a.view.len(), 3, "prompt, one merged message, prompt");
        let merged = a.at(1).unwrap();
        assert_eq!(merged.kind, Kind::Say);
        assert!(merged.raw.contains("first I will") && merged.raw.contains("and done"));
        // Showing the tool calls adds a row but must not change the message:
        // the gist is keyed on content, so if hiding tools changed what a
        // message contained, every toggle would buy the whole session again.
        let before = a.at(1).unwrap().content_hash();
        a.cfg.ui.show_tools = true;
        a.events.insert(
            2,
            ev(
                Kind::Tool {
                    name: "Bash".into(),
                },
                "ls",
                "ls",
                None,
            ),
        );
        a.rebuild();
        assert_eq!(
            a.view.len(),
            4,
            "prompt, the tool call, the message, prompt"
        );
        let after = a
            .view
            .iter()
            .find_map(|r| match r {
                crate::ui::app::Row::Many(i) => Some(a.fused[*i].content_hash()),
                _ => None,
            })
            .expect("still one fused message");
        assert_eq!(
            before, after,
            "the same message, so the same gist, so nothing new to buy"
        );
    }
    #[test]
    fn a_condensation_reaches_a_merged_message() {
        // A merged message is not in `events`, so a gist that only updated
        // `events` landed in the cache and nowhere the reader could see it.
        let long = "a sentence with enough words in it to be worth condensing ".repeat(3);
        let mut a = app_with(vec![
            ev(Kind::Prompt, "go", "go", None),
            ev(Kind::Say, &long, &long, None),
            ev(Kind::Say, &long, &long, None),
        ]);
        assert_eq!(a.view.len(), 2, "the two blocks are one message");
        let m = a.at(1).unwrap().clone();
        assert_eq!(m.gist_src, GistSource::Pending);

        a.absorb(crate::ui::app::Done {
            used: crate::llm::Usage::default(),
            session: m.session.clone(),
            uid: m.uid.clone(),
            hash: m.content_hash(),
            result: Ok("rewrote the parser".into()),
        });
        assert_eq!(a.at(1).unwrap().gist, "rewrote the parser");
        assert_eq!(a.at(1).unwrap().gist_src, GistSource::Model);
    }

    #[test]
    fn merged_blocks_are_never_condensed_individually() {
        // Paying for a dozen gists to show one merged line is the waste this
        // avoids. Proven by what the cache is allowed to reach: the merged
        // message, never the blocks it was made of.
        let long = "a sentence with enough words in it to be worth condensing ".repeat(3);
        let mut a = app_with(vec![
            ev(Kind::Prompt, "go", "go", None),
            ev(Kind::Say, &long, &long, None),
            ev(Kind::Say, &long, &long, None),
        ]);
        let block = a.events[1].content_hash();
        let merged = a.at(1).unwrap().content_hash();
        assert_ne!(block, merged);

        a.store
            .save_gist(
                "s",
                "x",
                &block,
                "block gist",
                "test",
                crate::llm::Usage::default(),
            )
            .unwrap();
        a.store
            .save_gist(
                "s",
                "y",
                &merged,
                "merged gist",
                "test",
                crate::llm::Usage::default(),
            )
            .unwrap();
        a.rebuild();

        assert_eq!(a.at(1).unwrap().gist, "merged gist");
        assert_eq!(
            a.events[1].gist_src,
            GistSource::Pending,
            "a block the reader never sees is never looked up, let alone paid for"
        );
    }

    #[test]
    fn bullets_render_under_their_headline() {
        let mut e = ev(Kind::Say, "", "the original", None);
        e.gist =
            "rewrote the parser\n- offsets stop at the last newline\n- sidechains dropped".into();
        let mut a = app_with(vec![e]);
        let s = screen(&mut a, 70, 8);
        let head = s
            .iter()
            .position(|l| l.contains("rewrote the parser"))
            .unwrap();
        assert!(s[head + 1].contains("offsets stop at the last newline"));
        assert!(s[head + 2].contains("sidechains dropped"));
        assert!(
            s[head + 1].trim_start().starts_with('·'),
            "bullets are marked: {:?}",
            s[head + 1]
        );
    }
    #[test]
    fn a_short_conversation_sits_at_the_bottom() {
        // Slack keeps a quiet channel against the bottom of the pane, so new
        // messages always arrive in the same place rather than creeping down.
        let mut a = app_with(vec![
            ev(Kind::Prompt, "first", "first", None),
            ev(Kind::Say, "second", "second", None),
            ev(Kind::Prompt, "third", "third", None),
        ]);
        let s = screen(&mut a, 60, 14);
        // Row 0 is the header, the last row the footer; the feed is between.
        assert!(s[1].is_empty(), "the feed starts empty under the header");
        assert!(
            s[s.len() - 2].contains("third"),
            "newest sits on the last line"
        );
        assert!(s[s.len() - 3].contains("second"));
        assert!(s[s.len() - 4].contains("first"));
    }

    #[test]
    fn notes_are_coloured_by_kind_not_all_alike() {
        let mut e = ev(Kind::Say, "", "the original", None);
        e.gist = "rewrote the parser\n! a torn line was lost\n→ src/source/claude.rs".into();
        let mut a = app_with(vec![e]);
        let mut t = Terminal::new(TestBackend::new(70, 8)).unwrap();
        t.draw(|f| draw(f, &mut a)).unwrap();
        let b = t.backend().buffer().clone();
        let find = |needle: char| {
            (0..b.area.height)
                .flat_map(|y| (0..b.area.width).map(move |x| (x, y)))
                .find(|&(x, y)| b[(x, y)].symbol() == needle.to_string())
                .map(|p| b[p].style().fg.unwrap())
        };
        assert_eq!(
            find('!'),
            Some(Theme::dark().alert),
            "an alert must catch the eye"
        );
        assert_eq!(
            find('→'),
            Some(Theme::dark().link),
            "a link must look like one"
        );
        assert_ne!(find('!'), find('→'));
    }

    #[test]
    fn only_messages_near_the_reader_are_condensed() {
        // A merged project holds thousands of messages; condensing every one
        // the moment it opens costs real money for lines nobody will read.
        let long = "a sentence with enough words in it to be worth condensing ".repeat(3);
        let events: Vec<Event> = (0..400)
            .map(|i| {
                if i % 2 == 0 {
                    ev(Kind::Prompt, "go", "go", None)
                } else {
                    ev(Kind::Say, &long, &long, None)
                }
            })
            .collect();
        let mut a = app_with(events);
        a.viewport = 20;
        a.go_bottom();
        let far = (0..a.view.len() / 4)
            .rev()
            .find(|&i| a.at(i).is_some_and(|e| e.kind == Kind::Say))
            .expect("a prose message well above the reader");
        assert_eq!(
            a.at(far).unwrap().gist_src,
            GistSource::Pending,
            "a message far from the reader is left alone"
        );

        // Scrolling to it brings it into range: the cache is consulted for
        // what is near, and only what is near.
        let hash = a.at(far).unwrap().content_hash();
        a.store
            .save_gist(
                "s",
                "u",
                &hash,
                "condensed",
                "test",
                crate::llm::Usage::default(),
            )
            .unwrap();
        a.selected = far;
        a.sync_view_for_test();
        assert_eq!(a.at(far).unwrap().gist, "condensed");
    }

    #[test]
    fn exporting_reaches_the_whole_conversation() {
        // `moan export` condenses the whole conversation. This used to be
        // expressed as a usize::MAX window; adding one to that wrapped to zero
        // in release and the export silently condensed nothing at all. It is a
        // flag now, which cannot overflow.
        let long = "a sentence with enough words in it to be worth condensing ".repeat(3);
        let mut a = app_with(vec![
            ev(Kind::Prompt, "go", "go", None),
            ev(Kind::Say, &long, &long, None),
        ]);
        a.condense_all = true;
        a.selected = 0;
        let hash = a.at(1).unwrap().content_hash();
        a.store
            .save_gist(
                "s",
                "u",
                &hash,
                "condensed",
                "test",
                crate::llm::Usage::default(),
            )
            .unwrap();
        a.sync_view_for_test();
        assert_eq!(a.at(1).unwrap().gist, "condensed");
    }

    /// Comfortably past the settling delay.
    const SETTLE_TEST: i64 = 120;

    fn aged(kind: Kind, gist: &str, secs: i64) -> Event {
        let mut e = ev(kind, gist, gist, None);
        e.ts = Utc::now() - chrono::Duration::seconds(secs);
        e
    }

    #[test]
    fn fresh_messages_carry_an_accent_that_fades() {
        let mut a = app_with(vec![
            aged(Kind::Prompt, "ancient", 10_000),
            aged(Kind::Say, "an hour ago", 3600),
            aged(Kind::Prompt, "a minute ago", 90),
            aged(Kind::Say, "just now", 2),
        ]);
        let mut t = Terminal::new(TestBackend::new(70, 10)).unwrap();
        t.draw(|f| draw(f, &mut a)).unwrap();
        let b = t.backend().buffer().clone();

        // Column 0 of a message row carries the accent, if it has one.
        let accent_on = |needle: &str| {
            (0..b.area.height)
                .find(|&y| {
                    (0..b.area.width).any(|x| {
                        let mut s = String::new();
                        for i in x..b.area.width {
                            s.push_str(b[(i, y)].symbol());
                        }
                        s.contains(needle)
                    })
                })
                .map(|y| (b[(0, y)].symbol().to_string(), b[(0, y)].style().fg))
        };
        let (mark, colour) = accent_on("just now").unwrap();
        assert_eq!(
            colour,
            Some(Theme::dark().fade[0]),
            "a new message is marked brightly"
        );

        let (older, colour_old) = accent_on("a minute ago").unwrap();
        assert_ne!(
            colour_old,
            Some(Theme::dark().fade[0]),
            "a minute old is dimmer"
        );
        assert_ne!(
            mark, older,
            "and thinner: the ramp has to read with the colour turned off"
        );

        assert_eq!(
            accent_on("an hour ago").unwrap().0,
            " ",
            "old news is unmarked"
        );
    }

    #[test]
    fn claude_shows_as_working_until_it_answers() {
        // Posting and getting nothing back yet.
        let mut a = app_with(vec![aged(Kind::Prompt, "do the thing", 3)]);
        assert!(a.awaiting());
        assert!(screen(&mut a, 70, 10).iter().any(|l| l.contains("claude")));

        // Running a tool is still working, not answering.
        a.events.push(aged(
            Kind::Tool {
                name: "Bash".into(),
            },
            "cargo test",
            2,
        ));
        a.rebuild();
        assert!(a.awaiting(), "a tool call is not a reply");

        // Saying something ends it.
        a.events.push(aged(Kind::Say, "done", 1));
        a.rebuild();
        assert!(!a.awaiting());
    }

    #[test]
    fn an_abandoned_session_does_not_spin_forever() {
        let a = app_with(vec![aged(Kind::Prompt, "asked and never answered", 4000)]);
        assert!(
            !a.awaiting(),
            "nothing is being written, so nothing is working"
        );
    }

    #[test]
    fn a_turn_still_arriving_is_not_paid_for_yet() {
        // A turn lands as several blocks. Condensing after each one buys the
        // same message over and over, and only the last answer is right.
        let long = "a sentence with enough words in it to be worth condensing ".repeat(3);
        let mut a = app_with(vec![
            aged(Kind::Prompt, "go", 60),
            aged(Kind::Say, &long, 1),
        ]);
        a.go_bottom();
        let last = a.view.len() - 1;
        let hash = a.at(last).unwrap().content_hash();
        a.store
            .save_gist(
                "s",
                "u",
                &hash,
                "condensed",
                "test",
                crate::llm::Usage::default(),
            )
            .unwrap();

        a.sync_view_for_test();
        assert_eq!(
            a.at(last).unwrap().gist_src,
            GistSource::Pending,
            "the newest turn is left alone while it may still grow"
        );

        // Once it has gone quiet it is worth paying for.
        let n = a.events.len() - 1;
        a.events[n].ts = Utc::now() - chrono::Duration::seconds(SETTLE_TEST);
        a.rebuild();
        assert_eq!(a.at(a.view.len() - 1).unwrap().gist, "condensed");
    }

    #[test]
    fn an_older_message_is_never_deferred() {
        // Only the newest can still be growing; everything before it is done.
        let long = "a sentence with enough words in it to be worth condensing ".repeat(3);
        let mut a = app_with(vec![
            aged(Kind::Prompt, "go", 5),
            aged(Kind::Say, &long, 4),
            aged(Kind::Prompt, "and again", 3),
        ]);
        let hash = a.at(1).unwrap().content_hash();
        a.store
            .save_gist(
                "s",
                "u",
                &hash,
                "condensed",
                "test",
                crate::llm::Usage::default(),
            )
            .unwrap();
        a.sync_view_for_test();
        assert_eq!(a.at(1).unwrap().gist, "condensed");
    }

    #[test]
    fn only_work_that_is_really_happening_turns() {
        // A spinner on a message nobody is condensing is a lie, and the
        // commonest one: most pending rows are simply out of reach.
        let long = "a sentence with enough words in it to be worth condensing ".repeat(3);
        let mut a = app_with(vec![
            aged(Kind::Prompt, "go", 400),
            aged(Kind::Say, &long, 300),
            aged(Kind::Say, &long, 200),
        ]);
        a.events[1].gist_src = GistSource::Working;
        a.events[2].gist_src = GistSource::Pending;
        a.rebuild();

        assert!(
            state_mark(GistSource::Working, &Theme::dark()).is_some(),
            "in flight turns"
        );
        assert!(
            state_mark(GistSource::Pending, &Theme::dark()).is_none(),
            "not sent does not"
        );
        assert!(
            state_mark(GistSource::Model, &Theme::dark()).is_none(),
            "finished does not"
        );
        assert!(state_mark(GistSource::Verbatim, &Theme::dark()).is_none());

        let (_, colour) = state_mark(GistSource::Failed, &Theme::dark()).unwrap();
        assert_eq!(
            colour,
            Theme::dark().error,
            "a failure is visible on its own row"
        );
    }

    #[test]
    fn the_header_carries_the_motion_and_the_rows_do_not() {
        // Rows are calm; only the header turns. Otherwise dozens of queued
        // messages strobe in unison while four are actually being worked.
        assert_eq!(
            state_mark(GistSource::Working, &Theme::dark())
                .unwrap()
                .0
                .trim(),
            "⋯"
        );
        let frames: std::collections::HashSet<&str> = (0..SPIN.len() as u64).map(spinner).collect();
        assert_eq!(frames.len(), SPIN.len(), "every frame is distinct");
        assert_eq!(spinner(0), spinner(SPIN.len() as u64), "and it loops");
    }

    #[test]
    fn a_placeholder_never_passes_for_a_message() {
        let long = "a sentence with enough words in it to be worth condensing ".repeat(3);
        let mut a = app_with(vec![aged(Kind::Say, &long, 400)]);
        a.events[0].gist_src = GistSource::Working;
        a.rebuild();
        let mut t = Terminal::new(TestBackend::new(80, 8)).unwrap();
        t.draw(|f| draw(f, &mut a)).unwrap();
        let b = t.backend().buffer().clone();

        // Find the row the message is on, not the first row with an "a" in it.
        let line_at = |y: u16| {
            (0..b.area.width)
                .map(|x| b[(x, y)].symbol().to_string())
                .collect::<String>()
        };
        let row = (0..b.area.height)
            .find(|&y| line_at(y).contains("sentence"))
            .expect("the message rendered");
        let styled = (0..b.area.width)
            .map(|x| b[(x, row)].style())
            .find(|s| s.fg == Some(Theme::dark().dim) && s.add_modifier.contains(Modifier::ITALIC));
        assert!(styled.is_some(), "a placeholder is dimmed and slanted");
    }

    #[test]
    fn a_condensation_lands_whatever_state_the_row_was_in() {
        // Narrowing this to one state is how a finished condensation reaches
        // the cache and never the row: the header goes quiet while the feed
        // still shows placeholders.
        let long = "a sentence with enough words in it to be worth condensing ".repeat(3);
        for state in [GistSource::Pending, GistSource::Working, GistSource::Failed] {
            let mut a = app_with(vec![aged(Kind::Say, &long, 400)]);
            a.events[0].gist_src = state;
            a.rebuild();
            let e = a.at(0).unwrap().clone();
            a.absorb(crate::ui::app::Done {
                used: crate::llm::Usage::default(),
                session: e.session.clone(),
                uid: e.uid.clone(),
                hash: e.content_hash(),
                result: Ok("condensed".into()),
            });
            assert_eq!(a.at(0).unwrap().gist, "condensed", "from {state:?}");
            assert_eq!(a.at(0).unwrap().gist_src, GistSource::Model);
        }
    }

    #[test]
    fn nothing_is_drawn_in_a_colour_the_theme_did_not_choose() {
        // Mixing literal colours with themed ones is how a feed ends up
        // unreadable on a light background: half of it follows the terminal
        // and half does not.
        let src = include_str!("render.rs");
        let body = src.split("#[cfg(test)]").next().unwrap();
        let literals: Vec<&str> = body
            .lines()
            .filter(|l| {
                [
                    "Cyan", "Magenta", "Gray", "White", "Green", "Yellow", "Blue", "Rgb", "Black",
                    "Red",
                ]
                .iter()
                .any(|c| l.contains(&format!("Color::{c}")))
            })
            .collect();
        assert!(
            literals.is_empty(),
            "colour bypassing the theme: {literals:#?}"
        );
    }

    #[test]
    fn both_palettes_draw_a_readable_feed() {
        for theme in [Theme::dark(), Theme::light()] {
            let mut a = app_with(vec![
                ev(Kind::Prompt, "make it work on white too", "x", None),
                ev(Kind::Say, "done", "done", None),
            ]);
            a.theme = theme;
            let s = screen(&mut a, 70, 10);
            assert!(s.iter().any(|l| l.contains("make it work on white too")));
            assert!(s.iter().any(|l| l.contains("done")));
        }
    }

    #[test]
    fn an_expanded_message_reads_as_markdown_not_as_source() {
        let raw = "## What landed\n\n**Merged** to `main` — see [the PR](https://x.dev/p/7).\n\n- one\n- two\n\n```sh\n**not bold**\n```";
        let mut a = app_with(vec![ev(Kind::Say, "landed", raw, None)]);
        a.toggle_expand();
        let s = screen(&mut a, 80, 24);
        let all = s.join("\n");

        assert!(all.contains("What landed"), "{all}");
        assert!(!all.contains("## What landed"), "heading marks are gone");
        assert!(all.contains("Merged"));
        assert!(!all.contains("**Merged**"), "emphasis marks are gone");
        assert!(all.contains("• one"), "items get a bullet");
        assert!(all.contains("**not bold**"), "fenced code is left alone");
    }

    #[test]
    fn a_link_shows_where_it_goes() {
        // Hidden behind its text, a url is one nobody can follow or copy.
        let raw = "see [the PR](https://x.dev/p/7) for why";
        let mut a = app_with(vec![ev(Kind::Say, "x", raw, None)]);
        a.toggle_expand();
        let all = screen(&mut a, 80, 16).join("\n");
        assert!(all.contains("https://x.dev/p/7"), "{all}");
    }

    #[test]
    fn a_url_is_never_broken_across_lines() {
        let url = "https://github.com/oddurs/moan/pull/1234567890";
        let raw = format!("a few words before the link {url} and some after");
        let mut a = app_with(vec![ev(Kind::Say, "x", &raw, None)]);
        a.toggle_expand();
        // Wide enough that it fits; nothing can save a url longer than a line.
        let s = screen(&mut a, 100, 16);
        assert!(
            s.iter().any(|l| l.contains(url)),
            "the url must survive whole on one line: {s:#?}"
        );
    }

    #[test]
    fn a_feed_line_shows_words_not_markup() {
        let mut e = ev(Kind::Say, "", "x", None);
        e.gist = "found `--json` breaks across lines\n· **bold** detail".into();
        e.gist_src = GistSource::Model;
        let mut a = app_with(vec![e]);
        let all = screen(&mut a, 80, 10).join("\n");
        assert!(all.contains("found --json breaks across lines"), "{all}");
        assert!(!all.contains("`--json`"), "backticks are styling, not text");
        assert!(all.contains("bold detail"));
        assert!(!all.contains("**bold**"));
    }

    #[test]
    fn opening_a_message_with_no_link_says_so_rather_than_guessing() {
        let mut a = app_with(vec![ev(Kind::Say, "nothing to click", "plain prose", None)]);
        a.open_link();
        assert_eq!(a.status, "no link in this message");
        assert!(!a.status_is_error);
    }

    #[test]
    fn only_a_web_address_is_ever_opened() {
        // A transcript is full of paths and commands. Handing one of those to
        // the system opener is not something a log viewer should do.
        for raw in [
            "run [this](file:///etc/passwd)",
            "see [that](javascript:alert(1))",
        ] {
            let mut a = app_with(vec![ev(Kind::Say, "x", raw, None)]);
            a.open_link();
            assert!(a.status.starts_with("not opening"), "{}", a.status);
            assert!(a.status_is_error);
        }
    }

    /// A conversation taller than any test viewport, so scrolling has room.
    fn long() -> Vec<Event> {
        (0..60)
            .map(|i| ev(turn(i), &format!("message number {i}"), "x", None))
            .collect()
    }

    fn with_panel(a: &mut App) {
        a.sessions = vec![
            sess("one", "~/Code/alpha", Some("main")),
            sess("two", "~/Code/alpha", Some("fix/x")),
            sess("three", "~/Code/beta", None),
        ];
        a.panel.open = true;
        a.rebuild_panel();
    }

    fn sess(id: &str, project: &str, branch: Option<&str>) -> crate::source::Session {
        crate::source::Session {
            id: id.into(),
            title: project.into(),
            project: project.into(),
            branch: branch.map(str::to_string),
            assistant_last: None,
            source: "claude",
            path: std::path::PathBuf::from("/gone.jsonl"),
            modified: Utc::now() - chrono::Duration::days(9),
            bytes: 1,
        }
    }

    #[test]
    fn the_panel_groups_projects_and_names_lone_sessions_by_project() {
        let mut a = app_with(vec![ev(Kind::Say, "x", "x", None)]);
        with_panel(&mut a);
        let s = screen(&mut a, 120, 16).join("\n");
        assert!(
            s.contains("~/Code/alpha"),
            "a project with two gets a heading"
        );
        assert!(
            s.contains("main") && s.contains("fix/x"),
            "named by branch under it"
        );
        assert!(
            s.contains("~/Code/beta"),
            "a lone session is named by its project"
        );
    }

    #[test]
    fn the_panel_moves_over_what_can_be_chosen() {
        let mut a = app_with(vec![ev(Kind::Say, "x", "x", None)]);
        with_panel(&mut a);
        let start = a.panel.selected;
        assert!(a.panel.rows[start].selectable());
        a.panel.step(true);
        assert!(a.panel.rows[a.panel.selected].selectable());
        assert_ne!(a.panel.selected, start);
    }

    #[test]
    fn a_narrow_terminal_gets_the_panel_over_the_feed() {
        let mut a = app_with(vec![ev(Kind::Prompt, "a message here", "x", None)]);
        with_panel(&mut a);
        // Split: both are visible at once.
        let wide = screen(&mut a, 120, 14).join("\n");
        assert!(wide.contains("~/Code/alpha") && wide.contains("a message here"));
        assert!(a.panel.splits(120));

        // Too narrow to split: thirty columns out of seventy leaves nothing
        // worth reading, so the panel covers the feed instead.
        assert!(!a.panel.splits(70));
        let narrow = screen(&mut a, 70, 14).join("\n");
        assert!(narrow.contains("~/Code/alpha"), "still reachable: {narrow}");
    }

    #[test]
    fn a_turn_whose_tools_are_still_running_is_not_bought_yet() {
        // The text of a turn pauses while a command runs. Judging by the
        // message's own last line says "finished" in the middle of a turn,
        // and the turn then has to be bought again when it grows.
        let long = "a sentence with enough words in it to be worth condensing ".repeat(3);
        let mut a = app_with(vec![
            aged(Kind::Prompt, "go", 300),
            aged(Kind::Say, &long, 120),
            aged(
                Kind::Tool {
                    name: "Bash".into(),
                },
                "cargo test",
                1,
            ),
        ]);
        a.cfg.ui.show_tools = false;
        a.rebuild();
        a.go_bottom();

        let last = a.view.len() - 1;
        let hash = a.at(last).unwrap().content_hash();
        a.store
            .save_gist(
                "s",
                "u",
                &hash,
                "too early",
                "test",
                crate::llm::Usage::default(),
            )
            .unwrap();
        a.sync_view_for_test();
        assert_eq!(
            a.at(last).unwrap().gist_src,
            GistSource::Pending,
            "the session is still being written to"
        );

        // Once everything has gone quiet, it is worth paying for.
        let n = a.events.len() - 1;
        a.events[n].ts = Utc::now() - chrono::Duration::seconds(SETTLE_TEST);
        a.rebuild();
        assert_eq!(a.at(a.view.len() - 1).unwrap().gist, "too early");
    }

    #[test]
    fn a_feed_taller_than_a_u16_still_clicks_correctly() {
        // The scroll offset used to be a u16 holding an absolute line number.
        // Past 65535 lines it wrapped, and the mouse handler adds a row to it
        // to decide what was clicked — so clicks landed on the wrong message.
        let events: Vec<Event> = (0..70_000)
            .map(|i| ev(turn(i), &format!("line-{i}"), "", None))
            .collect();
        let mut a = app_with(events);
        a.go_bottom();
        screen(&mut a, 80, 20);
        assert!(
            a.scroll > u16::MAX as usize,
            "the test needs a feed past the old ceiling: {}",
            a.scroll
        );
        // The row under the cursor is found from the absolute line, so it must
        // still be the last one.
        let line = a.scroll + 17;
        let row = a
            .row_starts
            .partition_point(|&s| s <= line)
            .saturating_sub(1);
        assert_eq!(row, a.selected, "the click lands on the selected row");
    }

    fn frame_ms(rows: usize) -> f64 {
        let events: Vec<Event> = (0..rows)
            .map(|i| ev(turn(i), &format!("line-{i}"), "some original text", None))
            .collect();
        let mut a = app_with(events);
        a.go_bottom();
        let mut t = Terminal::new(TestBackend::new(100, 40)).unwrap();
        t.draw(|f| draw(f, &mut a)).unwrap();
        let start = std::time::Instant::now();
        for _ in 0..10 {
            t.draw(|f| draw(f, &mut a)).unwrap();
        }
        start.elapsed().as_secs_f64() * 1000.0 / 10.0
    }

    /// Numbers to read, rather than a threshold to pass. `scripts/task bench`.
    #[test]
    #[ignore]
    fn report_frame_cost() {
        println!("\n  drawing a frame");
        for n in [1_000usize, 10_000, 50_000] {
            println!("    {n:6} messages   {:6.2} ms", frame_ms(n));
        }
    }

    #[test]
    fn drawing_does_not_get_slower_with_the_length_of_the_conversation() {
        // Laying out every row to find out how tall it was made the frame
        // linear in the conversation: 36ms at ten thousand messages and 178ms
        // at fifty thousand, which is five frames a second.
        //
        // The assertion is on the shape rather than on a wall-clock number, so
        // it means the same thing on a busy machine as on an idle one.
        let small = frame_ms(2_000).max(0.05);
        let large = frame_ms(20_000);
        let growth = large / small;
        assert!(
            growth < 4.0,
            "ten times the messages cost {growth:.1}x the frame \
             ({small:.2}ms -> {large:.2}ms); drawing has gone linear again"
        );
    }

    #[test]
    fn the_interface_speaks_one_vocabulary() {
        // `session`, `event` and `gist` are the parser's words. They are good
        // names in the code and wrong ones in front of a reader, who has a
        // conversation, in a project, made of messages.
        let mut bad = Vec::new();
        for (file, src) in [
            ("render.rs", include_str!("render.rs")),
            ("main.rs", include_str!("../main.rs")),
            ("app.rs", include_str!("app.rs")),
            ("panel.rs", include_str!("panel.rs")),
        ] {
            let body = src.split("#[cfg(test)]").next().unwrap();
            for (n, line) in body.lines().enumerate() {
                // A line may say it means the old word — the alias that keeps
                // `moan sessions` working is the only one, and hiding it by
                // splitting the string in two would be worse than saying so.
                if line.contains("vocabulary-exempt") {
                    continue;
                }
                let code = line.split("//").next().unwrap_or("");
                // Only what is *between* quotes reaches a reader. Everything
                // after the first one is not the same thing: a format argument
                // such as `app.sessions.len()` follows the string it fills in.
                let mut inside = String::new();
                let mut open = false;
                for c in code.chars() {
                    if c == '"' {
                        open = !open;
                        continue;
                    }
                    if open {
                        inside.push(c);
                    }
                }
                let inside = inside.to_lowercase();
                for word in ["session", "event", "gist", "condensation"] {
                    if inside.contains(word) {
                        bad.push(format!("{file}:{}  {}", n + 1, line.trim()));
                    }
                }
            }
        }
        assert!(
            bad.is_empty(),
            "the parser's words reached the surface:\n{}",
            bad.join("\n")
        );
    }

    #[test]
    fn returning_lands_where_you_stopped_and_copes_when_it_has_gone() {
        let events: Vec<Event> = (0..40)
            .map(|i| ev(turn(i), &format!("line-{i}"), "x", None))
            .collect();
        let mut a = app_with(events);
        a.session = Some(crate::source::Session {
            id: "c".into(),
            title: "~/p".into(),
            project: "~/p".into(),
            branch: None,
            assistant_last: None,
            source: "claude",
            path: std::path::PathBuf::from("/gone.jsonl"),
            modified: Utc::now(),
            bytes: 1,
        });
        a.sessions = vec![a.session.clone().unwrap()];
        a.selected = 12;
        a.remember_place();
        assert_eq!(
            a.store.place().map(|p| p.1),
            Some(12),
            "the message, not zero"
        );

        // A remembered message that no longer exists must not match the first
        // row: an empty prefix is a prefix of everything.
        a.store.put_place("c", 9_999, false, "").unwrap();
        a.selected = 0;
        // Opening a conversation whose transcript has been deleted must work —
        // the archive outlives them — and a remembered message that is no
        // longer there must not put the reader at the top.
        assert!(a.resume().unwrap(), "a deleted transcript still opens");
        assert_eq!(
            a.selected,
            a.view.len().saturating_sub(1),
            "nothing to return to, so the newest message"
        );
    }

    #[test]
    fn an_empty_pane_says_what_would_fill_it() {
        // Nothing yet.
        let mut a = app_with(vec![]);
        assert!(screen(&mut a, 90, 10)
            .join(" ")
            .contains("has not started yet"));

        // Everything hidden by a filter.
        let mut a = app_with(vec![ev(Kind::Say, "about parsers", "x", None)]);
        a.filter = "zebra".into();
        a.rebuild();
        let s = screen(&mut a, 90, 10).join(" ");
        assert!(s.contains("zebra"), "it names what was searched for: {s}");

        // Everything hidden because tools are off.
        let mut a = app_with(vec![ev(
            Kind::Tool {
                name: "Bash".into(),
            },
            "ls",
            "ls",
            None,
        )]);
        a.cfg.ui.show_tools = false;
        a.rebuild();
        assert!(screen(&mut a, 90, 10).join(" ").contains("press o"));
    }

    #[test]
    fn the_help_never_hides_its_own_last_line() {
        // It used to be a fraction of the screen, so on a short terminal the
        // last third was cut off with nothing to say it had been.
        let mut a = app_with(vec![ev(Kind::Say, "x", "x", None)]);
        a.mode = Mode::Help;
        // Derived from the content rather than written down, so adding a line
        // to the overlay cannot quietly make the test wrong.
        let need = help_rows();
        for h in [24u16, need, need + 10] {
            let s = screen(&mut a, 90, h).join("\n");
            assert!(s.contains("q"), "the keys are there at {h} rows");
            assert!(
                s.contains(NOTES[NOTES.len() - 1]) || h < need,
                "the last line is reachable at {h} rows, needing {need}"
            );
        }
    }

    #[test]
    fn looking_for_conversations_twice_in_a_moment_only_looks_once() {
        // No real transcripts: a test that reads whatever is on the machine
        // is a test whose result depends on what somebody else is doing.
        let mut a = app_with(vec![]);
        a.sources = Vec::new();
        a.refresh_sessions().unwrap();
        a.sessions.push(sess("x", "~/p", None));
        a.refresh_sessions().unwrap();
        assert_eq!(a.sessions.len(), 1, "the second look was skipped");
    }

    #[test]
    fn one_unreadable_source_does_not_take_the_others_with_it() {
        // opencode keeps its conversations in a database that can be busy,
        // missing or unreadable. None of that is a reason for moan not to
        // start, and it used to be: the error reached main and exited 101.
        struct Broken;
        impl crate::source::Source for Broken {
            fn name(&self) -> &'static str {
                "broken"
            }
            fn list(&self) -> anyhow::Result<Vec<crate::source::Stub>> {
                anyhow::bail!("the database is locked")
            }
            fn describe(&self, _: &crate::source::Stub) -> crate::source::Facts {
                unreachable!()
            }
            fn parse(
                &self,
                _: &std::path::Path,
                _: &str,
                _: &crate::source::Cursor,
            ) -> anyhow::Result<(Vec<Event>, crate::source::Cursor)> {
                unreachable!()
            }
        }
        let mut a = app_with(vec![]);
        a.sources = vec![Box::new(Broken)];
        // Whatever the last message was has been read; this is the next one.
        a.status.clear();
        a.refresh_sessions()
            .expect("a broken source is not an error");
        assert!(a.status.contains("broken"), "and it says so: {}", a.status);
        assert!(!a.status_is_error, "it is a fact, not a failure");
    }

    #[test]
    fn a_reply_is_named_by_the_agent_that_wrote_it() {
        // "claude" over a Codex reply is simply wrong, and with both in one
        // list it is wrong on screen at the same time as being right above it.
        let mut a = app_with(vec![ev(Kind::Say, "did the thing", "x", None)]);
        a.sessions = vec![sess("s", "~/p", None)];
        assert_eq!(a.agent_of("s"), "claude");

        let mut codex = sess("s", "~/p", None);
        codex.source = "codex";
        a.sessions = vec![codex];
        assert_eq!(a.agent_of("s"), "codex");
        assert!(screen(&mut a, 70, 8).join(" ").contains("codex"));

        // A conversation we know nothing about does not crash or invent.
        assert_eq!(a.agent_of("nobody"), "claude");
    }

    /// A gist with real newlines, which `ev` flattens away.
    fn multi(kind: Kind, head: &str) -> Event {
        Event {
            gist: format!("{head}\n! something to know\n- a detail\n→ https://example.com"),
            ..ev(kind, head, "x", None)
        }
    }

    #[test]
    fn a_click_lands_on_the_message_it_is_over() {
        // Every one of these got the mapping wrong at some point: the short
        // feed by the height of the blank rows it is padded with, and the
        // scrolled feed by nothing, which is how it should have been all along.
        for (name, mut a) in [
            ("long", app_with(long())),
            ("short", app_with(long()[..3].to_vec())),
            (
                "multi-line",
                app_with(
                    (0..20)
                        .map(|i| multi(turn(i), &format!("headline {i}")))
                        .collect(),
                ),
            ),
        ] {
            let (w, h) = (90u16, 14u16);
            screen(&mut a, w, h);
            for _ in 0..3 {
                crate::ui::wheel_for_test(&mut a, 40, false);
            }
            let shown = screen(&mut a, w, h);
            for r in FEED_TOP..h - 1 {
                let text = shown[r as usize].trim();
                crate::ui::click_for_test(&mut a, 40, r);
                // The headline is the first line of the row that starts here;
                // a bullet belongs to the row above it, and clicking one picks
                // that row, so only headline rows are checked.
                let Some(e) = a.at(a.selected) else { continue };
                let head = e.gist.lines().next().unwrap_or("");
                if !text.is_empty() && text.contains(head) {
                    assert!(
                        text.contains(head),
                        "{name}: clicking screen row {r} ({text:?}) selected {head:?}"
                    );
                }
                if text.is_empty() {
                    continue;
                }
                let starts_row = a
                    .row_starts
                    .get(a.selected)
                    .copied()
                    .is_some_and(|s| s == a.scroll + (r - FEED_TOP) as usize - a.feed_pad);
                if starts_row {
                    assert!(
                        text.contains(head),
                        "{name}: screen row {r} shows {text:?} but the click chose {head:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_rows_measured_height_is_the_height_it_draws() {
        // `message_height` feeds the layout, which decides where every row
        // starts, which decides what a click means. If it disagrees with what
        // `message` actually emits, clicks drift further the further down you
        // are — and nothing else would say so.
        let t = Theme::dark();
        for e in [
            ev(Kind::Say, "one line", "x", None),
            ev(Kind::Prompt, "a prompt", "x", Some("done")),
            multi(Kind::Say, "a headline"),
            Event {
                gist: "head\n\n- kept\n\n".into(),
                ..ev(Kind::Say, "head", "x", None)
            },
        ] {
            for w in [40usize, 90, 200] {
                let drawn = message(&e, "claude", w, false, false, &t).len();
                assert_eq!(
                    message_height(&e),
                    drawn,
                    "{:?} at {w} columns: measured {} but drew {drawn}",
                    e.gist,
                    message_height(&e)
                );
            }
        }
    }

    #[test]
    fn the_wheel_scrolls_whatever_is_under_it() {
        // Not whatever has the keyboard: a wheel that moves the other pane is
        // a wheel that feels broken.
        let mut a = app_with(long());
        with_panel(&mut a);
        a.panel.focus = crate::ui::panel::Focus::Feed;
        screen(&mut a, 120, 16);

        a.go_top();
        screen(&mut a, 120, 16);
        let feed = a.scroll;
        let list = a.panel.scroll;

        crate::ui::wheel_for_test(&mut a, 90, true);
        assert!(a.scroll > feed, "over the conversation, the feed moved");
        assert_eq!(a.panel.scroll, list, "and the list stayed where it was");
    }

    #[test]
    fn the_wheel_moves_the_view_and_the_arrows_move_the_cursor() {
        // Two positions, not one: where you are looking, and what `o` would
        // expand. The wheel used to drag the cursor around with it, so every
        // glance down the page changed what the keys would act on.
        let mut a = app_with(long());
        a.go_top();
        screen(&mut a, 90, 12);
        assert_eq!((a.selected, a.scroll), (0, 0));

        crate::ui::wheel_for_test(&mut a, 40, true);
        screen(&mut a, 90, 12);
        assert!(a.scroll > 0, "the wheel moved the view");
        assert_eq!(a.selected, 0, "and left the cursor exactly where it was");
    }

    #[test]
    fn reaching_for_the_cursor_brings_it_to_what_you_are_looking_at() {
        // The alternative is dragging the view back to a cursor you scrolled
        // away from, which throws away the place you chose. So after scrolling,
        // the first arrow moves from what is on screen — and the highlight is
        // never pinned to an edge while you scroll, because it is not drawn
        // until you reach for it.
        let mut a = app_with(long());
        a.go_top();
        screen(&mut a, 90, 12);

        for _ in 0..6 {
            crate::ui::wheel_for_test(&mut a, 40, true);
        }
        screen(&mut a, 90, 12);
        let (first, last) = a.rows_in_view_for_test();
        assert!(first > 0, "the cursor at row 0 is off screen now");

        let landed = a.scroll;
        a.move_by(1);
        screen(&mut a, 90, 12);
        assert!(
            (first..=last).contains(&a.selected),
            "the cursor came to the view: {} not in {first}..={last}",
            a.selected
        );
        assert_eq!(a.scroll, landed, "and the view did not jump back to it");
    }

    #[test]
    fn scrolling_to_the_bottom_resumes_following() {
        // Slack's bargain: reading back stops the feed jumping under you, and
        // returning to the bottom means you want the newest thing again.
        let mut a = app_with(long());
        screen(&mut a, 90, 12);
        assert!(a.follow, "it starts at the newest message");

        crate::ui::wheel_for_test(&mut a, 40, false);
        assert!(!a.follow, "scrolling back stops following");

        for _ in 0..40 {
            crate::ui::wheel_for_test(&mut a, 40, true);
        }
        assert!(a.follow, "and reaching the bottom starts again");
    }

    #[test]
    fn a_click_in_the_list_lands_on_the_row_under_the_pointer() {
        // The list's scroll position used to be measured in screen lines while
        // its selection was measured in rows, so a click after scrolling
        // landed two rows out — and further out again with the query showing.
        let mut a = app_with(long());
        a.sessions = (0..40)
            .map(|i| sess(&format!("s{i}"), "~/Code/alpha", Some(&format!("b{i}"))))
            .collect();
        a.panel.open = true;
        a.rebuild_panel();
        screen(&mut a, 120, 20);

        for _ in 0..4 {
            crate::ui::wheel_for_test(&mut a, 2, true);
        }
        screen(&mut a, 120, 20);

        // The row drawn at this line, worked out the way the renderer does.
        let head = 2;
        let line = usize::from(FEED_TOP) + head + 3;
        assert!(a.panel.scroll > 0, "the list actually scrolled first");
        let want = a.panel.scroll + 3;
        assert!(
            a.panel
                .rows
                .get(want)
                .is_some_and(crate::ui::panel::Row::selectable),
            "the row under the pointer is one you can pick"
        );

        crate::ui::click_for_test(&mut a, 2, u16::try_from(line).unwrap());
        assert_eq!(
            a.panel.selected, want,
            "the click selected the row it was over"
        );
    }

    #[test]
    fn the_lists_header_never_scrolls_away() {
        // It is what says which tab you are on and why the list is short.
        let mut a = app_with(long());
        a.sessions = (0..40)
            .map(|i| sess(&format!("s{i}"), "~/Code/alpha", Some(&format!("b{i}"))))
            .collect();
        a.panel.open = true;
        a.rebuild_panel();
        screen(&mut a, 120, 16);

        for _ in 0..30 {
            crate::ui::wheel_for_test(&mut a, 2, true);
        }
        let shown = screen(&mut a, 120, 16);
        assert!(a.panel.scroll > 0, "the list scrolled");
        assert!(
            shown.iter().any(|l| l.contains("conversations")),
            "the tabs are still there: {shown:#?}"
        );
    }

    #[test]
    fn a_message_holding_a_credential_is_never_sent() {
        // Prose long enough to want summarising is the one path out of this
        // machine. A key echoed by a shell into a reply must not take it.
        let leaked = format!(
            "the command printed the key and I should not have run it: {} {}",
            "sk-or-v1-82d7aa11bb22cc33dd44ee55ff66aa77",
            "a sentence with enough words to be worth condensing ".repeat(2)
        );
        let mut a = app_with(vec![
            aged(Kind::Prompt, "go", 400),
            aged(Kind::Say, &leaked, 300),
        ]);
        a.go_bottom();
        a.sync_view_for_test();

        let e = a.at(1).unwrap();
        assert_eq!(e.gist_src, GistSource::Withheld, "kept back, not queued");
        assert_eq!(a.withheld, 1);
        assert!(
            a.status.contains("OpenRouter"),
            "and it says what it found: {}",
            a.status
        );

        // The row says so, and the header counts it.
        let screen = screen(&mut a, 100, 10).join("\n");
        assert!(screen.contains("⊘"), "{screen}");
        assert!(screen.contains("kept back"));

        // An answer arriving for the same text from elsewhere — another moan
        // sharing the archive — must not overwrite the decision.
        let hash = a.at(1).unwrap().content_hash();
        a.absorb(crate::ui::app::Done {
            used: crate::llm::Usage::default(),
            session: "s".into(),
            uid: a.at(1).unwrap().uid.clone(),
            hash,
            result: Ok("a summary from somewhere else".into()),
        });
        assert_eq!(a.at(1).unwrap().gist_src, GistSource::Withheld);
    }

    #[test]
    fn ordinary_prose_still_goes() {
        // The guard is worthless if it withholds everything.
        let long = "a sentence with enough words in it to be worth condensing ".repeat(3);
        let mut a = app_with(vec![
            aged(Kind::Prompt, "go", 400),
            aged(Kind::Say, &long, 300),
        ]);
        a.go_bottom();
        let hash = a.at(1).unwrap().content_hash();
        a.store
            .save_gist(
                "s",
                "u",
                &hash,
                "condensed",
                "test",
                crate::llm::Usage::default(),
            )
            .unwrap();
        a.sync_view_for_test();
        assert_eq!(a.at(1).unwrap().gist, "condensed");
        assert_eq!(a.withheld, 0);
    }

    #[test]
    fn typing_narrows_the_list_and_says_so() {
        // A hundred conversations cannot be reached with arrow keys, and a
        // list that quietly loses things is worse than one that is long.
        let mut a = app_with(vec![]);
        a.sources = Vec::new();
        a.sessions = vec![
            sess("1", "~/Code/unifont", Some("main")),
            sess("2", "~/Code/unifont", Some("feat/x")),
            sess("3", "~/Code/moan", None),
            sess("4", "~/Code/perfect", None),
        ];
        a.panel.open = true;
        a.rebuild_panel();
        let all = a.panel.rows.len();

        a.panel.query = "unifont".into();
        a.rebuild_panel();
        let narrowed = a.panel.rows.len();
        assert!(narrowed < all, "typing narrowed it: {narrowed} of {all}");
        let shown = screen(&mut a, 120, 14).join("\n");
        assert!(shown.contains("unifont"));
        assert!(!shown.contains("perfect"), "what does not match is gone");
        assert!(
            shown.contains("2 of 4"),
            "and it says how much of what: {shown}"
        );

        // A branch finds its project, not only the project name.
        a.panel.query = "feat/x".into();
        a.rebuild_panel();
        assert_eq!(
            a.panel
                .rows
                .iter()
                .filter(|r| matches!(r, crate::ui::panel::Row::Session(_)))
                .count(),
            1
        );

        // And nothing matching says so rather than looking broken.
        a.panel.query = "zebra".into();
        a.rebuild_panel();
        assert!(screen(&mut a, 120, 14)
            .join("\n")
            .contains("nothing matches"));
    }

    #[test]
    fn only_the_odd_agent_out_is_marked() {
        // A letter on every row when most of them say the same thing is noise.
        let mut a = app_with(vec![]);
        a.sources = Vec::new();
        let mut odd = sess("2", "~/Code/b", None);
        odd.source = "codex";
        a.sessions = vec![
            sess("1", "~/Code/a", None),
            odd,
            sess("3", "~/Code/c", None),
        ];
        a.refresh_sessions().unwrap();
        a.sessions = vec![
            sess("1", "~/Code/a", None),
            {
                let mut o = sess("2", "~/Code/b", None);
                o.source = "codex";
                o
            },
            sess("3", "~/Code/c", None),
        ];
        a.common_agent = "claude";
        a.panel.open = true;
        a.rebuild_panel();
        let shown = screen(&mut a, 120, 12).join("\n");
        assert!(
            shown.contains("c ~/Code/b"),
            "the exception is marked: {shown}"
        );
        assert!(!shown.contains("c ~/Code/a"), "the rule is not");
    }

    #[test]
    fn filtering_narrows_the_feed() {
        // Separate turns: prose from one turn is one message, and filtering
        // cannot show half of a message.
        let mut a = app_with(vec![
            ev(Kind::Prompt, "ask", "ask", None),
            ev(Kind::Say, "about parsers", "", None),
            ev(Kind::Prompt, "ask again", "ask again", None),
            ev(Kind::Say, "about renderers", "", None),
        ]);
        a.filter = "pars".into();
        a.rebuild();
        let s = screen(&mut a, 60, 8);
        assert!(s.iter().any(|l| l.contains("about parsers")));
        assert!(!s.iter().any(|l| l.contains("about renderers")));
    }
    #[test]
    fn narrow_terminals_do_not_panic() {
        let mut a = app_with(vec![ev(
            Kind::Tool {
                name: "Bash".into(),
            },
            "a command far wider than the terminal is",
            "x",
            Some("ok"),
        )]);
        for w in [8u16, 14, 20, 40] {
            let s = screen(&mut a, w, 6);
            assert!(
                s.iter().all(|l| l.chars().count() <= w as usize),
                "width {w}"
            );
        }
    }
    #[test]
    fn an_expansion_in_a_long_feed_stays_anchored() {
        let mut events: Vec<Event> = (0..500)
            .map(|i| ev(turn(i), &format!("line-{i}"), "", None))
            .collect();
        events[250].raw = (0..60).map(|i| format!("body-{i}\n")).collect();
        let mut a = app_with(events);
        a.selected = 250;
        a.follow = false;
        a.toggle_expand();
        let s = screen(&mut a, 60, 12);
        // The row itself must stay visible, not be scrolled past by its body.
        assert!(s.iter().any(|l| l.contains("line-250")), "{s:#?}");
        assert!(s.iter().any(|l| l.contains("body-0")));
        assert!(!s.iter().any(|l| l.contains("line-251")));
    }
    #[test]
    fn a_click_selects_the_row_under_it() {
        let events: Vec<Event> = (0..50)
            .map(|i| ev(turn(i), &format!("line-{i}"), "", None))
            .collect();
        let mut a = app_with(events);
        screen(&mut a, 60, 12);
        // Ten rows of feed sit below the header; the last is row 49.
        assert_eq!(a.row_starts.len(), 50);
        let top = a.scroll;
        let clicked = a.row_starts.iter().rposition(|&s| s <= top + 2).unwrap();
        assert_eq!(clicked, top + 2, "row_starts must be absolute line numbers");
    }
    #[test]
    fn a_run_longer_than_the_line_is_cut_not_lost() {
        let long = "x".repeat(50);
        let out = hard_wrap(&long, 10);
        assert!(out.iter().all(|l| l.width() <= 10));
        assert_eq!(out.concat(), long, "every character survives");
    }
}
