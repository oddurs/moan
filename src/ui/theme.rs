//! Colour.
//!
//! Two rules. Anything that stands for a *role* — who spoke, whether something
//! went wrong — uses an ANSI colour, so it comes from the palette the reader
//! already chose for their terminal and sits with everything else on screen.
//! Anything that is a *surface* — a selection, a rule, the fade on a new
//! message — has to know whether it is being drawn on black or on white, and
//! so is defined twice.
//!
//! The exception is Claude, which is orange because that is Claude's colour,
//! not because of where it falls in a palette.

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Work it out from the terminal, and assume dark if it will not say.
    #[default]
    Auto,
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// Chrome that should recede: rules, keys, the quiet half of a row.
    pub dim: Color,
    /// Body text that is neither dim nor emphasised.
    pub text: Color,
    /// The strongest foreground, for what the eye should land on.
    pub bright: Color,
    /// Behind the selected message.
    pub selection: Color,
    /// The scrollbar, resting and active.
    pub track: Color,
    pub thumb: Color,
    /// A message's age, newest first.
    pub fade: [Color; 3],

    /// Who is speaking.
    pub user: Color,
    pub claude: Color,
    pub tool: Color,
    pub system: Color,

    /// What a note is telling you.
    pub alert: Color,
    pub link: Color,
    pub error: Color,

    /// Badges — the name in the header, a panel title.
    pub badge_fg: Color,
    pub badge_bg: Color,
    /// The branch, and the chip for the session you are on.
    pub branch: Color,
    pub live: Color,
}

// The sixteen colours the terminal already has. Naming them by index rather
// than by `Color::Red` makes it plain that these are the reader's colours, not
// ours: whatever their scheme puts at 5 is what "claude" will be.
const BLACK: Color = Color::Indexed(0);
const RED: Color = Color::Indexed(1);
const GREEN: Color = Color::Indexed(2);
const YELLOW: Color = Color::Indexed(3);
const BLUE: Color = Color::Indexed(4);
const MAGENTA: Color = Color::Indexed(5);
const CYAN: Color = Color::Indexed(6);
const WHITE: Color = Color::Indexed(7);
const GREY: Color = Color::Indexed(8);
const BR_GREEN: Color = Color::Indexed(10);
const BR_BLUE: Color = Color::Indexed(12);
const BR_WHITE: Color = Color::Indexed(15);

impl Theme {
    /// A dark terminal.
    ///
    /// Index 0 rather than a fixed dark grey for surfaces: in a dark scheme it
    /// is a shade lifted off the background, which is exactly what a selection
    /// wants to be, and it is the reader's shade rather than a guess. On
    /// London Moquette that is `#303F61` against a `#1E2941` ground.
    pub fn dark() -> Self {
        Self {
            dim: GREY,
            text: WHITE,
            bright: BR_WHITE,
            selection: BLACK,
            track: BLACK,
            thumb: GREY,
            // Arrival, fading: bright green, green, then the same grey as
            // everything else that has stopped mattering. Three steps that all
            // come from the scheme, so they sit in its palette rather than
            // beside it.
            fade: [BR_GREEN, GREEN, GREY],
            user: CYAN,
            claude: MAGENTA,
            tool: BLUE,
            system: GREY,
            alert: YELLOW,
            link: BR_BLUE,
            error: RED,
            badge_fg: BLACK,
            badge_bg: MAGENTA,
            branch: CYAN,
            live: GREEN,
        }
    }

    /// A pale terminal. The same roles; the surfaces swap ends.
    pub fn light() -> Self {
        Self {
            dim: GREY,
            text: BLACK,
            bright: BLACK,
            // On a white ground it is index 7 that sits just off it.
            selection: WHITE,
            track: WHITE,
            thumb: GREY,
            fade: [GREEN, BR_GREEN, GREY],
            user: CYAN,
            claude: MAGENTA,
            tool: BLUE,
            system: GREY,
            alert: YELLOW,
            link: BLUE,
            error: RED,
            badge_fg: BR_WHITE,
            badge_bg: MAGENTA,
            branch: CYAN,
            live: GREEN,
        }
    }

    pub fn of(mode: Mode) -> Self {
        match mode {
            Mode::Dark => Self::dark(),
            Mode::Light => Self::light(),
            Mode::Auto if detect_light() => Self::light(),
            Mode::Auto => Self::dark(),
        }
    }
}

/// Is the terminal's background pale?
///
/// `COLORFGBG` is the only thing a terminal reliably leaves in the environment
/// — its second field is the background's palette index. Querying the terminal
/// itself over OSC 11 would be surer, but it means writing to the tty and
/// waiting for a reply before the first frame, and a terminal that ignores the
/// query leaves that wait hanging. Dark is the safer guess when nothing says.
fn detect_light() -> bool {
    let Ok(v) = std::env::var("COLORFGBG") else {
        return false;
    };
    let Some(bg) = v.rsplit(';').next() else {
        return false;
    };
    // 7 and 15 are the two whites; everything else is a dark ground.
    matches!(bg.trim(), "7" | "15")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_role_keeps_its_meaning_in_both_palettes() {
        let (d, l) = (Theme::dark(), Theme::light());
        // Every role is distinguishable from its neighbours in both.
        for t in [d, l] {
            let roles = [t.user, t.claude, t.tool, t.alert, t.error];
            for (i, a) in roles.iter().enumerate() {
                for b in &roles[i + 1..] {
                    assert_ne!(a, b, "two roles share a colour");
                }
            }
        }
    }

    #[test]
    fn surfaces_swap_ends_between_palettes() {
        let (d, l) = (Theme::dark(), Theme::light());
        // A selection drawn to sit just off black is invisible on white. What
        // has to differ is which end of the ramp each surface takes — not the
        // grey in the middle, which serves both.
        assert_ne!(d.selection, l.selection);
        assert_ne!(d.text, l.text);
        assert_ne!(d.bright, l.bright);
        assert_eq!(d.dim, l.dim, "the mid grey is legible on either ground");
    }

    #[test]
    fn every_colour_is_one_the_terminal_chose() {
        // A hardcoded RGB ignores the reader's scheme, which is how a feed
        // ends up with muddy greens on a blue ground.
        let named = |c: Color| matches!(c, Color::Indexed(i) if i < 16);
        for t in [Theme::dark(), Theme::light()] {
            for (what, c) in [
                ("dim", t.dim),
                ("text", t.text),
                ("bright", t.bright),
                ("selection", t.selection),
                ("track", t.track),
                ("thumb", t.thumb),
                ("user", t.user),
                ("claude", t.claude),
                ("tool", t.tool),
                ("system", t.system),
                ("alert", t.alert),
                ("link", t.link),
                ("error", t.error),
                ("badge_fg", t.badge_fg),
                ("badge_bg", t.badge_bg),
                ("branch", t.branch),
                ("live", t.live),
                ("fade0", t.fade[0]),
                ("fade1", t.fade[1]),
                ("fade2", t.fade[2]),
            ] {
                assert!(named(c), "{what} is {c:?}, not a colour from the scheme");
            }
        }
    }

    #[test]
    fn the_fade_really_fades() {
        for t in [Theme::dark(), Theme::light()] {
            assert_ne!(t.fade[0], t.fade[1]);
            assert_ne!(t.fade[1], t.fade[2]);
        }
    }

    #[test]
    fn a_pale_background_is_recognised() {
        // COLORFGBG is "fg;bg", sometimes "fg;default;bg".
        for (v, light) in [
            ("15;0", false),
            ("0;15", true),
            ("0;7", true),
            ("15;default;0", false),
            ("0;default;15", true),
            ("", false),
        ] {
            std::env::set_var("COLORFGBG", v);
            assert_eq!(detect_light(), light, "COLORFGBG={v:?}");
        }
        std::env::remove_var("COLORFGBG");
    }
}
