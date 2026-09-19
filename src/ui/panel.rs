//! The side panel: everything running, beside what you are reading.
//!
//! It is the only surface for moving between conversations: everything on
//! the machine, grouped by project, beside what you are reading rather than
//! over it.

use crate::source::Session;

/// Which pane the keyboard is acting on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Feed,
    Panel,
}

/// What the panel is showing.
///
/// Adding one means adding a variant and a line to `MODES`. The layout, the
/// focus handling and the keys are written against the list rather than
/// against any particular member of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Sessions,
    Search,
}

/// Every mode, in the order they are shown and cycled.
pub const MODES: &[(Mode, &str)] = &[(Mode::Sessions, "conversations"), (Mode::Search, "search")];

impl Mode {
    pub fn title(self) -> &'static str {
        MODES
            .iter()
            .find(|(m, _)| *m == self)
            .map_or("", |(_, name)| *name)
    }

    /// The next mode along, wrapping.
    pub fn step(self, forward: bool) -> Self {
        let i = MODES.iter().position(|(m, _)| *m == self).unwrap_or(0);
        let n = MODES.len();
        let next = if forward { i + 1 } else { i + n - 1 };
        MODES[next % n].0
    }

    /// Does this mode take typing?
    pub fn takes_input(self) -> bool {
        matches!(self, Mode::Search)
    }
}

/// One line of the panel. Only `Session` and `Result` can be selected.
#[derive(Debug, Clone)]
pub enum Row {
    /// A project heading, and how many sessions sit under it.
    Project {
        title: String,
        count: usize,
    },
    /// An index into `App::sessions`.
    Session(usize),
    /// An index into `Panel::hits`.
    Result(usize),
    Gap,
}

impl Row {
    pub fn selectable(&self) -> bool {
        matches!(self, Row::Session(_) | Row::Result(_))
    }
}

/// How a session is doing, at a glance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Written to just now: an agent is working.
    Live,
    /// The assistant has stopped and nobody has replied.
    Waiting,
    /// Neither.
    Idle,
}

impl State {
    /// Legible without colour, because colour alone is not a signal.
    pub fn mark(self) -> &'static str {
        match self {
            State::Live => "●",
            State::Waiting => "◆",
            State::Idle => " ",
        }
    }
}

/// Below this the panel covers the feed instead of sitting beside it: thirty
/// columns taken out of eighty leaves nothing worth reading.
pub const MIN_SPLIT: u16 = 92;
pub const MIN_WIDTH: u16 = 24;
pub const MAX_WIDTH: u16 = 48;

#[derive(Debug, Clone)]
pub struct Panel {
    pub open: bool,
    pub focus: Focus,
    pub mode: Mode,
    pub selected: usize,
    pub width: u16,
    pub rows: Vec<Row>,
    /// What the search box holds, which is not the feed's filter.
    pub query: String,
    pub typing: bool,
    /// What the query found, in the order shown.
    pub hits: Vec<crate::store::Hit>,
    /// How many rows the list can draw, from the last frame.
    pub shown: usize,
    /// First row drawn. The wheel moves this; the selection does not.
    pub scroll: usize,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            open: false,
            focus: Focus::Feed,
            mode: Mode::Sessions,
            selected: 0,
            width: 30,
            rows: Vec::new(),
            query: String::new(),
            typing: false,
            hits: Vec::new(),
            scroll: 0,
            shown: 20,
        }
    }
}

impl Panel {
    /// Does the panel sit beside the feed, or over it?
    pub fn splits(&self, total: u16) -> bool {
        self.open && total >= MIN_SPLIT
    }

    pub fn columns(&self, total: u16) -> u16 {
        self.width.clamp(MIN_WIDTH, MAX_WIDTH).min(total / 2)
    }

    /// Move the selection to the next selectable row in `step` direction.
    pub fn step(&mut self, down: bool) {
        let candidates: Box<dyn Iterator<Item = usize>> = if down {
            Box::new(self.selected + 1..self.rows.len())
        } else {
            Box::new((0..self.selected).rev())
        };
        for i in candidates {
            if self.rows[i].selectable() {
                self.selected = i;
                self.reveal();
                return;
            }
        }
    }

    /// Move the list without moving the selection.
    ///
    /// Same bargain as the feed: the wheel changes what you can see, the
    /// arrows change what enter would open. The selection is only ever
    /// touched to stop it hiding off an edge.
    pub fn scroll_by(&mut self, d: i64) {
        let floor = self
            .rows
            .len()
            .saturating_sub(self.shown.min(self.rows.len()));
        self.scroll = if d < 0 {
            self.scroll.saturating_sub(d.unsigned_abs() as usize)
        } else {
            self.scroll.saturating_add(d.unsigned_abs() as usize)
        }
        .min(floor);
        self.carry();
    }

    /// Pull the list the shortest distance that shows the selection.
    pub fn reveal(&mut self) {
        if self.shown == 0 {
            return;
        }
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + self.shown {
            self.scroll = self.selected + 1 - self.shown;
        }
    }

    /// Keep the selection on a row the list still shows.
    fn carry(&mut self) {
        if self.shown == 0 || self.rows.is_empty() {
            return;
        }
        let top = self.scroll;
        let bottom = (self.scroll + self.shown).min(self.rows.len());
        if (top..bottom).contains(&self.selected) {
            return;
        }
        // Toward whichever edge it went out of, and only as far as a row that
        // can actually hold a selection — a project heading cannot.
        if let Some(near) = (top..bottom)
            .filter(|i| self.rows[*i].selectable())
            .min_by_key(|i| i.abs_diff(self.selected))
        {
            self.selected = near;
        }
    }

    /// Land on something selectable, for when the rows have just changed.
    pub fn settle(&mut self) {
        if self.rows.get(self.selected).is_some_and(Row::selectable) {
            self.reveal();
            return;
        }
        self.selected = self.rows.iter().position(Row::selectable).unwrap_or(0);
        self.reveal();
    }

    pub fn current(&self) -> Option<&Row> {
        self.rows.get(self.selected)
    }

    /// The first row currently drawn. A click is measured from here.
    pub fn first_shown(&self) -> usize {
        self.scroll
    }

    /// Move to another mode, leaving the selection somewhere real.
    pub fn switch(&mut self, to: Mode) {
        self.mode = to;
        self.typing = to.takes_input() && self.query.is_empty();
        self.selected = 0;
        self.scroll = 0;
    }
}

/// How a session is doing, from its file and its last event.
///
/// `waiting` means the assistant stopped and the human has not answered — the
/// inverse of the composing indicator, which knows when it is still working.
pub fn state(s: &Session, last_was_assistant: Option<bool>) -> State {
    let quiet = (chrono::Utc::now() - s.modified).num_seconds();
    if quiet < 120 {
        return State::Live;
    }
    match last_was_assistant {
        Some(true) if quiet < 60 * 60 * 24 => State::Waiting,
        _ => State::Idle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<Row> {
        vec![
            Row::Project {
                title: "p".into(),
                count: 2,
            },
            Row::Session(0),
            Row::Session(1),
            Row::Gap,
            Row::Project {
                title: "q".into(),
                count: 1,
            },
            Row::Session(2),
        ]
    }

    #[test]
    fn movement_skips_what_cannot_be_chosen() {
        let mut p = Panel {
            rows: rows(),
            selected: 1,
            ..Default::default()
        };
        p.step(true);
        assert_eq!(p.selected, 2);
        // Past the gap and the next heading, onto the session below them.
        p.step(true);
        assert_eq!(p.selected, 5);
        p.step(true);
        assert_eq!(p.selected, 5, "and stops at the end rather than wrapping");
        p.step(false);
        assert_eq!(p.selected, 2);
    }

    #[test]
    fn a_rebuilt_list_lands_somewhere_real() {
        let mut p = Panel {
            rows: rows(),
            selected: 0,
            ..Default::default()
        };
        p.settle();
        assert_eq!(p.selected, 1, "not on the heading it started on");

        p.rows = vec![Row::Project {
            title: "p".into(),
            count: 0,
        }];
        p.settle();
        assert_eq!(p.selected, 0, "nothing selectable is not a crash");
    }

    #[test]
    fn a_narrow_terminal_gets_an_overlay_not_a_split() {
        let p = Panel {
            open: true,
            ..Default::default()
        };
        assert!(!p.splits(80), "eighty columns cannot spare thirty");
        assert!(p.splits(120));
        assert!(!Panel::default().splits(200), "closed is closed");
    }

    #[test]
    fn the_panel_never_takes_more_than_half() {
        let p = Panel {
            open: true,
            width: 48,
            ..Default::default()
        };
        assert_eq!(p.columns(200), 48);
        assert_eq!(p.columns(60), 30, "half of sixty");
        let narrow = Panel {
            open: true,
            width: 10,
            ..Default::default()
        };
        assert_eq!(narrow.columns(200), MIN_WIDTH, "and never uselessly thin");
    }
}
