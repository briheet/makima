//! Draws the chrome into a ratatui buffer and encodes it as ANSI.
//!
//! Colours are ANSI palette indexes only, so the terminal theme decides how
//! everything looks and the chrome always matches the agents' terminals.

use makima_core::AgentId;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Widget};

use crate::ansi;
use crate::flow::NewWorkFlow;
use crate::keys::{KeyMenu, KeyStyle, Keymap};
use crate::layout::pane_areas;
use crate::picker::Picker;
use crate::state::{ChromeMode, ChromeState, FloatSpec, PaneRole, PaneSpec, PaneStatus};

pub(crate) const ACCENT: Color = Color::Blue;
pub(crate) const MUTED: Color = Color::DarkGray;
const SMALLEST: (u16, u16) = (24, 6);

/// Where an agent's live terminal goes, in cells (inside its border).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PaneRect {
    pub id: AgentId,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl PaneRect {
    #[must_use]
    pub fn new(id: AgentId, x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            id,
            x,
            y,
            width,
            height,
        }
    }
}

/// The columns a tab occupies in the tab line, for mouse clicks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct TabHit {
    pub index: usize,
    pub start: u16,
    pub end: u16,
}

/// A rectangle in cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct CellRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

/// Part of the chrome that sits over the agents' live terminals (a key
/// menu, the help, the picker, a tool's border), drawn on its own so the host
/// can put it above them.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Overlay {
    pub rect: CellRect,
    /// Bytes that draw just this rectangle, on a terminal its size.
    pub ansi: String,
}

/// A rendered chrome.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Frame {
    /// Bytes that draw the whole chrome on a terminal of the given size.
    pub ansi: String,
    /// Live terminals to place; panes not listed (zoomed away) are hidden.
    pub panes: Vec<PaneRect>,
    pub tabs: Vec<TabHit>,
    /// Where the open tool's terminal goes, inside its floating border.
    pub float: Option<CellRect>,
    /// What must show above the agents' terminals.
    pub overlay: Option<Overlay>,
}

/// Renders `state` for a `columns` × `rows` terminal. With a `flow`, the
/// new-work flow fills the body instead of the panes.
#[must_use]
pub fn render(
    state: &ChromeState,
    flow: Option<&NewWorkFlow>,
    picker: Option<&Picker>,
    columns: u16,
    rows: u16,
) -> Frame {
    let area = Rect::new(0, 0, columns, rows);
    let mut buffer = Buffer::empty(area);
    let (panes, tabs, cursor, float, covers) = draw(state, flow, picker, area, &mut buffer);
    let overlay = covers.into_iter().reduce(Rect::union).map(|rect| Overlay {
        rect: CellRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        },
        ansi: ansi::encode(&crop(&buffer, rect), cursor_in(cursor, rect)),
    });
    Frame {
        ansi: ansi::encode(&buffer, cursor),
        panes,
        tabs,
        float,
        overlay,
    }
}

/// `rect` of `buffer` as a buffer of its own, starting at (0, 0).
fn crop(buffer: &Buffer, rect: Rect) -> Buffer {
    let mut out = Buffer::empty(Rect::new(0, 0, rect.width, rect.height));
    for y in 0..rect.height {
        for x in 0..rect.width {
            out[(x, y)] = buffer[(rect.x + x, rect.y + y)].clone();
        }
    }
    out
}

/// `cursor`, relative to `rect`, if it's inside it.
fn cursor_in(cursor: Option<(u16, u16)>, rect: Rect) -> Option<(u16, u16)> {
    cursor
        .filter(|&(x, y)| rect.contains(ratatui::layout::Position::new(x, y)))
        .map(|(x, y)| (x - rect.x, y - rect.y))
}

/// What [`draw`] placed: pane rects, tab hits, the cursor, the tool's
/// rect, and the areas drawn over the panes.
type Drawn = (
    Vec<PaneRect>,
    Vec<TabHit>,
    Option<(u16, u16)>,
    Option<CellRect>,
    Vec<Rect>,
);

/// Draws into `buffer`.
pub(crate) fn draw(
    state: &ChromeState,
    flow: Option<&NewWorkFlow>,
    picker: Option<&Picker>,
    area: Rect,
    buffer: &mut Buffer,
) -> Drawn {
    if area.width < SMALLEST.0 || area.height < SMALLEST.1 {
        Paragraph::new("window too small")
            .style(Style::new().fg(MUTED))
            .render(area, buffer);
        return (Vec::new(), Vec::new(), None, None, Vec::new());
    }
    let tab_row = Rect::new(0, 0, area.width, 1);
    let body = body_of(area);
    let talk_row = Rect::new(0, area.height - 2, area.width, 1);
    let status_row = Rect::new(0, area.height - 1, area.width, 1);

    let tabs = draw_tabs(state, tab_row, buffer);
    // With no agents the splash is the whole screen and holds the prompt;
    // otherwise the prompt lives in the talk line under the panes.
    let (panes, mut cursor) = if let Some(flow) = flow {
        (Vec::new(), flow.draw(body, buffer))
    } else if state.panes.is_empty() {
        (Vec::new(), draw_empty(state, body, buffer))
    } else {
        let panes = draw_panes(state, body, buffer);
        (panes, draw_talk(state, talk_row, buffer))
    };
    draw_status(state, status_row, buffer);
    let mut covers = Vec::new();
    if let Some(picker) = picker {
        cursor = picker.draw(body, buffer);
        covers.push(picker.area(body));
    }
    let float = state.float.as_ref().map(|float| {
        // The tool's terminal shows its own cursor.
        cursor = None;
        let (outer, inner) = draw_float(float, body, buffer);
        covers.push(outer);
        inner
    });
    // The chrome's own menus and help sit above everything, tools too.
    covers.extend(draw_overlays(state, body, buffer));
    (panes, tabs, cursor, float, covers)
}

/// Everything between the tab line and the talk line.
fn body_of(area: Rect) -> Rect {
    Rect::new(0, 1, area.width, area.height.saturating_sub(3))
}

/// The key menu and the help overlay; returns where they went.
fn draw_overlays(state: &ChromeState, body: Rect, buffer: &mut Buffer) -> Vec<Rect> {
    let mut drawn = Vec::new();
    if let Some(menu) = &state.menu {
        drawn.push(draw_menu(menu, body, buffer));
    }
    if state.show_help {
        drawn.push(draw_help(state, body, buffer));
    }
    drawn
}

/// A tool's floating window: a rounded border with its title, folder and
/// how to hide it, centred over the body. Returns its area and where its
/// terminal goes.
fn draw_float(float: &FloatSpec, body: Rect, buffer: &mut Buffer) -> (Rect, CellRect) {
    let area = if float.zoomed {
        body
    } else {
        let percent = u32::from(float.percent.clamp(30, 100));
        let scale = |n: u16| u16::try_from(u32::from(n) * percent / 100).unwrap_or(n);
        let (width, height) = (scale(body.width), scale(body.height));
        Rect::new(
            body.x + (body.width - width) / 2,
            body.y + (body.height - height) / 2,
            width,
            height,
        )
    };
    Clear.render(area, buffer);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(ACCENT))
        .title_top(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                float.title.clone(),
                Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" · {} ", float.folder), Style::new().fg(MUTED)),
        ]))
        .title_top(
            Line::from(Span::styled(
                format!(" {} ", float.hint),
                Style::new().fg(MUTED),
            ))
            .right_aligned(),
        );
    let inner = block.inner(area);
    block.render(area, buffer);
    (
        area,
        CellRect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: inner.height,
        },
    )
}

fn draw_tabs(state: &ChromeState, row: Rect, buffer: &mut Buffer) -> Vec<TabHit> {
    let mut spans = vec![Span::raw(" ")];
    let mut hits = Vec::new();
    let mut column: u16 = 1;
    for (index, tab) in state.tabs.iter().enumerate() {
        let marker = if tab.active {
            "*"
        } else if tab.attention {
            "!"
        } else {
            ""
        };
        let label = format!(" {}:{}{marker} ", index + 1, tab.label);
        let style = if tab.active {
            Style::new().add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else if tab.attention {
            Style::new().fg(Color::Yellow)
        } else {
            Style::new().fg(MUTED)
        };
        let width = u16::try_from(label.chars().count()).unwrap_or(u16::MAX);
        hits.push(TabHit {
            index,
            start: column,
            end: column.saturating_add(width),
        });
        column = column.saturating_add(width + 1);
        spans.push(Span::styled(label, style));
        spans.push(Span::raw(" "));
    }
    Paragraph::new(Line::from(spans)).render(row, buffer);
    Paragraph::new(Span::styled("makima ", Style::new().fg(MUTED)))
        .alignment(Alignment::Right)
        .render(row, buffer);
    hits
}

fn draw_panes(state: &ChromeState, body: Rect, buffer: &mut Buffer) -> Vec<PaneRect> {
    let visible: Vec<&PaneSpec> = match &state.zoomed {
        Some(id) => state.panes.iter().filter(|p| p.id == *id).collect(),
        None => state.panes.iter().collect(),
    };
    pane_areas(body, visible.len())
        .into_iter()
        .zip(visible)
        .map(|(area, pane)| {
            // Under a tool, every pane is quiet.
            let focused = pane.focused && state.float.is_none();
            let block = pane_block(pane, focused);
            let inner = block.inner(area);
            block.render(area, buffer);
            PaneRect {
                id: pane.id,
                x: inner.x,
                y: inner.y,
                width: inner.width,
                height: inner.height,
            }
        })
        .collect()
}

/// `pane`'s border and titles; `focused` overrides the pane's own flag
/// (a tool on top quiets every pane).
fn pane_block(pane: &PaneSpec, focused: bool) -> Block<'_> {
    let (status_label, status_color) = status_label(pane.status);
    let border = if pane.status == PaneStatus::NeedsInput {
        Color::Yellow
    } else if focused {
        ACCENT
    } else {
        MUTED
    };
    let name_style = if focused {
        Style::new().add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(MUTED)
    };
    let role = match pane.role {
        PaneRole::Lead => Span::styled("◆ lead", Style::new().fg(ACCENT)),
        PaneRole::Child(number) => Span::styled(format!("{number}"), Style::new().fg(MUTED)),
    };
    let mut title = vec![
        Span::raw(" "),
        role,
        Span::styled(" · ", Style::new().fg(MUTED)),
        Span::styled(pane.name.as_str(), name_style),
    ];
    if pane.unguarded {
        title.push(Span::styled(
            " · ⚠ bypass",
            Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    }
    if !pane.detail.is_empty() {
        title.push(Span::styled(" · ", Style::new().fg(MUTED)));
        title.push(Span::styled(pane.detail.as_str(), Style::new().fg(MUTED)));
    }
    title.push(Span::raw(" "));
    Block::bordered()
        .border_type(if focused {
            BorderType::Thick
        } else {
            BorderType::Plain
        })
        .border_style(Style::new().fg(border))
        .title_top(Line::from(title))
        .title_top(
            Line::from(vec![
                Span::raw(" "),
                Span::styled(status_label, Style::new().fg(status_color)),
                Span::raw(" "),
            ])
            .right_aligned(),
        )
}

fn status_label(status: PaneStatus) -> (&'static str, Color) {
    match status {
        PaneStatus::Starting => ("starting …", MUTED),
        PaneStatus::Working => ("working ●", Color::Green),
        PaneStatus::NeedsInput => ("needs input ?", Color::Yellow),
        PaneStatus::Done => ("done ✓", Color::Blue),
        PaneStatus::Failed => ("failed ✗", Color::Red),
        PaneStatus::Exited => ("exited", MUTED),
    }
}

/// "MAKIMA" in the ANSI Shadow figlet font: `█` is the face, the box
/// characters are its shadow.
const LOGO: [&str; 6] = [
    "███╗   ███╗ █████╗ ██╗  ██╗██╗███╗   ███╗ █████╗ ",
    "████╗ ████║██╔══██╗██║ ██╔╝██║████╗ ████║██╔══██╗",
    "██╔████╔██║███████║█████╔╝ ██║██╔████╔██║███████║",
    "██║╚██╔╝██║██╔══██║██╔═██╗ ██║██║╚██╔╝██║██╔══██║",
    "██║ ╚═╝ ██║██║  ██║██║  ██╗██║██║ ╚═╝ ██║██║  ██║",
    "╚═╝     ╚═╝╚═╝  ╚═╝╚═╝  ╚═╝╚═╝╚═╝     ╚═╝╚═╝  ╚═╝",
];
const LOGO_WIDTH: u16 = 49;

/// The logo as lines: the block letters (bold face, dim shadow) when `big`,
/// else the word.
pub(crate) fn draw_logo(big: bool) -> Vec<Line<'static>> {
    if !big {
        return vec![Line::from(Span::styled(
            "makima",
            Style::new().add_modifier(Modifier::BOLD),
        ))];
    }
    LOGO.iter().map(|row| logo_line(row)).collect()
}

/// One row of the logo as runs of face and shadow, borrowing the row.
fn logo_line(row: &'static str) -> Line<'static> {
    let face = Style::new().add_modifier(Modifier::BOLD);
    let shadow = Style::new().fg(MUTED);
    let mut spans = Vec::new();
    let mut start = 0;
    let mut run_is_face = None;
    for (at, c) in row.char_indices() {
        let is_face = c == '█';
        if run_is_face.is_some_and(|f| f != is_face) {
            let style = if is_face { shadow } else { face };
            spans.push(Span::styled(&row[start..at], style));
            start = at;
        }
        run_is_face = Some(is_face);
    }
    if let Some(is_face) = run_is_face {
        spans.push(Span::styled(
            &row[start..],
            if is_face { face } else { shadow },
        ));
    }
    Line::from(spans)
}

/// Example commands and what they do, under the prompt.
const EXAMPLES: [(&str, &str); 4] = [
    ("new auth --lead claude --children 3", "start a team"),
    ("new \"fix ci\" --lead shell", "one shell"),
    ("help", "all commands"),
    ("? in Makima's keys", "all keys"),
];

/// The splash: logo, tagline, the `makima>` prompt in a box, and examples,
/// as one column centred in `body`. Returns the prompt's cursor.
fn draw_empty(state: &ChromeState, body: Rect, buffer: &mut Buffer) -> Option<(u16, u16)> {
    let dim = Style::new().fg(MUTED);
    let column_width = 60.min(body.width.saturating_sub(4));
    let notice_height = u16::from(state.notice.is_some());
    // logo, gap, tagline, gap, box (3), notice, gap, examples
    let height_with = |logo: u16| logo + 1 + 1 + 1 + 3 + notice_height + 1 + 4;
    let big_logo = column_width >= LOGO_WIDTH && body.height >= height_with(6);
    let height = height_with(if big_logo { 6 } else { 1 });
    let x = body.x + body.width.saturating_sub(column_width) / 2;
    let mut y = body.y + body.height.saturating_sub(height) / 2;
    // Rows past the body are dropped, never drawn over the status line.
    let line = |y: u16, content: Line, buffer: &mut Buffer| {
        if y < body.bottom() {
            Paragraph::new(content).render(Rect::new(x, y, column_width, 1), buffer);
        }
    };

    for logo_line in draw_logo(big_logo) {
        line(y, logo_line, buffer);
        y += 1;
    }
    y += 1;
    line(
        y,
        Line::from(Span::styled("run teams of coding agents side by side", dim)),
        buffer,
    );
    y += 2;

    let cursor = draw_prompt_box(
        state,
        Rect::new(x, y, column_width, 3).intersection(body),
        buffer,
    );
    y += 3;

    if let Some(notice) = &state.notice {
        line(
            y,
            Line::from(Span::styled(notice.clone(), Style::new().fg(Color::Red))),
            buffer,
        );
        y += 1;
    }
    y += 1;

    let command_width = EXAMPLES
        .iter()
        .map(|(command, _)| command.chars().count())
        .max()
        .unwrap_or(0)
        + 4;
    for (command, what) in EXAMPLES {
        line(
            y,
            Line::from(vec![
                Span::raw(format!("{command:<command_width$}")),
                Span::styled(what, dim),
            ]),
            buffer,
        );
        y += 1;
    }
    cursor
}

/// The `makima>` prompt in a box: highlighted while typing, quiet
/// otherwise. Returns the cursor while typing.
fn draw_prompt_box(state: &ChromeState, input: Rect, buffer: &mut Buffer) -> Option<(u16, u16)> {
    let typing = state.mode == ChromeMode::Command;
    let block =
        Block::bordered().border_style(Style::new().fg(if typing { ACCENT } else { MUTED }));
    let inner = block.inner(input);
    block.render(input, buffer);
    if inner.height == 0 {
        return None;
    }
    let prompt = Span::styled(
        "makima> ",
        Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
    );
    let prompt_width = prompt.width();
    let text_width = usize::from(inner.width).saturating_sub(prompt_width + 2);
    let skip = state.talk.cursor.saturating_sub(text_width);
    let visible: String = state
        .talk
        .text
        .chars()
        .skip(skip)
        .take(text_width)
        .collect();
    Paragraph::new(Line::from(vec![Span::raw(" "), prompt, Span::raw(visible)]))
        .render(Rect::new(inner.x, inner.y, inner.width, 1), buffer);
    typing
        .then(|| u16::try_from(1 + prompt_width + state.talk.cursor - skip).ok())
        .flatten()
        .map(|column| (inner.x + column, inner.y))
}

/// Draws the talk line; returns where the cursor goes when typing.
fn draw_talk(state: &ChromeState, row: Rect, buffer: &mut Buffer) -> Option<(u16, u16)> {
    let typing = matches!(state.mode, ChromeMode::Talk | ChromeMode::Command);
    if typing {
        let prompt = Span::styled(
            format!(" {}", state.talk.prompt),
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
        );
        let prompt_width = prompt.width();
        let text_width = usize::from(row.width).saturating_sub(prompt_width + 1);
        // Scroll the text so the cursor stays visible.
        let skip = state.talk.cursor.saturating_sub(text_width);
        let visible: String = state
            .talk
            .text
            .chars()
            .skip(skip)
            .take(text_width)
            .collect();
        Paragraph::new(Line::from(vec![prompt, Span::raw(visible)])).render(row, buffer);
        let column = prompt_width + state.talk.cursor - skip;
        return u16::try_from(column).ok().map(|x| (row.x + x, row.y));
    }
    let line = match &state.notice {
        Some(notice) => Line::from(Span::styled(
            format!(" {notice}"),
            Style::new().fg(Color::Red),
        )),
        None => key_hints(state),
    };
    Paragraph::new(line).render(row, buffer);
    None
}

/// The line under the panes: how to reach Makima's keys from where you
/// are, keys in the accent colour so they stand out from the words.
fn key_hints(state: &ChromeState) -> Line<'_> {
    use HintKey::{Fixed, Leader};

    let tmux = state.key_style == KeyStyle::Tmux;
    let leader = state.leader.as_str();
    let hints: &[(HintKey, &str)] = match (state.mode, tmux) {
        (ChromeMode::Keys, false) => &[
            (Fixed("i"), "back to the agent"),
            (Fixed("space g"), "git"),
            (Fixed("space s"), "shell"),
            (Fixed("space o"), "all tools"),
            (Fixed("[ ]"), "works"),
            (Fixed("space"), "work menu"),
            (Fixed("?"), "all keys"),
        ],
        (ChromeMode::Keys, true) => &[
            (Fixed("g"), "git"),
            (Fixed("s"), "shell"),
            (Fixed("T"), "all tools"),
            (Fixed("w"), "pick work"),
            (Fixed("[ ]"), "works"),
            (Fixed("?"), "all keys"),
            (Fixed("esc"), "back"),
        ],
        (ChromeMode::Tool, _) => &[(
            Leader,
            "hides it, then space g/s/o for another tool or i for the agent",
        )],
        (_, false) => &[
            (Fixed("C-hjkl"), "move between agents"),
            (Leader, "Makima's keys"),
            (Fixed("then space g"), "git"),
            (Fixed("space o"), "all tools"),
            (Fixed("?"), "all keys"),
        ],
        (_, true) => &[
            (Fixed("C-hjkl"), "move between agents"),
            (Leader, "then a key"),
            (Fixed("g"), "git"),
            (Fixed("T"), "tools"),
            (Fixed("?"), "all keys"),
        ],
    };
    let key_style = Style::new().fg(ACCENT).add_modifier(Modifier::BOLD);
    let muted = Style::new().fg(MUTED);
    let mut spans = Vec::with_capacity(hints.len() * 4 + 1);
    spans.push(Span::raw(" "));
    for (i, &(keys, what)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", muted));
        }
        let keys = match keys {
            Fixed(keys) => keys,
            Leader => leader,
        };
        spans.push(Span::styled(keys, key_style));
        spans.push(Span::styled(" ", muted));
        spans.push(Span::styled(what, muted));
    }
    Line::from(spans)
}

/// The keys of one hint: written out, or the configured leader.
#[derive(Clone, Copy)]
enum HintKey {
    Fixed(&'static str),
    Leader,
}

fn draw_status(state: &ChromeState, row: Rect, buffer: &mut Buffer) {
    let helix = state.key_style == KeyStyle::Helix;
    let (badge, color) = match state.mode {
        ChromeMode::Pane if helix => ("INS", MUTED),
        ChromeMode::Pane => ("NORMAL", MUTED),
        ChromeMode::Keys if helix => ("NOR", ACCENT),
        ChromeMode::Keys => ("PREFIX", Color::Yellow),
        ChromeMode::Picker => ("PICK", ACCENT),
        ChromeMode::Talk => ("TALK", ACCENT),
        ChromeMode::Command => ("COMMAND", Color::Magenta),
        ChromeMode::Confirm => ("CONFIRM", Color::Red),
        ChromeMode::NewWork => ("NEW WORK", ACCENT),
        ChromeMode::Tool => ("TOOL", Color::Magenta),
    };
    let separator = || Span::styled(" │ ", Style::new().fg(MUTED));
    let mut spans = vec![Span::styled(
        format!(" {badge} "),
        Style::new()
            .fg(Color::Black)
            .bg(color)
            .add_modifier(Modifier::BOLD),
    )];
    if let Some(float) = &state.float {
        spans.push(Span::styled(
            format!(" {}", float.title),
            Style::new().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        ));
        spans.push(separator());
    }
    if let Some(work) = &state.work_name {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            work.clone(),
            Style::new().add_modifier(Modifier::BOLD),
        ));
    }
    let counts = state.counts;
    let mut first = true;
    for (count, label, color) in [
        (counts.working, "● {} working", Color::Green),
        (counts.waiting, "? {} waiting", Color::Yellow),
        (counts.done, "✓ {} done", Color::Blue),
        (counts.failed, "✗ {} failed", Color::Red),
    ] {
        if count == 0 {
            continue;
        }
        spans.push(if first { separator() } else { Span::raw("  ") });
        first = false;
        spans.push(Span::styled(
            label.replace("{}", &count.to_string()),
            Style::new().fg(color),
        ));
    }
    let right = match (&state.confirm, state.mode) {
        (Some(question), ChromeMode::Confirm) => Line::from(Span::styled(
            format!("{question} (y/n) "),
            Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
        _ => Line::from(vec![
            Span::styled(format!("{} ? keys", state.leader), Style::new().fg(MUTED)),
            separator(),
            Span::raw(format!("{} ", state.clock)),
        ]),
    };
    // The right side wins when space is short; the left is clipped before it.
    let right_width = u16::try_from(right.width() + 1)
        .unwrap_or(u16::MAX)
        .min(row.width);
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(right_width)]).areas(row);
    Paragraph::new(Line::from(spans)).render(left_area, buffer);
    Paragraph::new(right)
        .alignment(Alignment::Right)
        .render(right_area, buffer);
}

/// Every key sequence of the active key map, in two columns when tall.
fn draw_help(state: &ChromeState, body: Rect, buffer: &mut Buffer) -> Rect {
    let style = state.key_style;
    let default_bindings;
    let bindings = if state.bindings.is_empty() {
        default_bindings = Keymap::new(style).help();
        &default_bindings
    } else {
        &state.bindings
    };
    let title = if style == KeyStyle::Tmux {
        format!(" keys after {} ", state.leader)
    } else {
        format!(" keys in NOR ({}; i to type) ", state.leader)
    };
    let columns = if bindings.len() > usize::from(body.height.saturating_sub(4)) {
        2
    } else {
        1
    };
    let per_column = bindings.len().div_ceil(columns);
    let column_width: u16 = 34;
    let width = (column_width * u16::try_from(columns).unwrap_or(1) + 4).min(body.width);
    let height = u16::try_from(per_column)
        .unwrap_or(u16::MAX)
        .saturating_add(2)
        .min(body.height);
    let area = Rect::new(
        body.x + body.width.saturating_sub(width) / 2,
        body.y + body.height.saturating_sub(height) / 2,
        width,
        height,
    );
    Clear.render(area, buffer);
    let block = Block::bordered()
        .border_style(Style::new().fg(ACCENT))
        .title_top(Line::from(title));
    let inner = block.inner(area);
    block.render(area, buffer);
    for (i, (keys, what)) in bindings.iter().enumerate() {
        let column = u16::try_from(i / per_column).unwrap_or(0);
        let row = u16::try_from(i % per_column).unwrap_or(0);
        let x = inner.x + 1 + column * column_width;
        if row >= inner.height || x >= inner.right() {
            continue;
        }
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{keys:<9}"),
                Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::raw(what.as_str()),
        ]))
        .render(
            Rect::new(x, inner.y + row, column_width.min(inner.right() - x), 1),
            buffer,
        );
    }
    area
}

/// A minor mode's keys in a box at the bottom right, like Helix's infobox.
fn draw_menu(menu: &KeyMenu, body: Rect, buffer: &mut Buffer) -> Rect {
    let width = 30.min(body.width);
    let height = u16::try_from(menu.entries.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2)
        .min(body.height);
    let area = Rect::new(
        body.right().saturating_sub(width + 1),
        body.bottom().saturating_sub(height),
        width,
        height,
    );
    Clear.render(area, buffer);
    let lines: Vec<Line> = menu
        .entries
        .iter()
        .map(|(key, what)| {
            Line::from(vec![
                Span::styled(
                    format!(" {key:<6}"),
                    Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::raw(what.as_str()),
            ])
        })
        .collect();
    Paragraph::new(lines)
        .block(
            Block::bordered()
                .border_style(Style::new().fg(MUTED))
                .title_top(Line::from(format!(" {} ", menu.title))),
        )
        .render(area, buffer);
    area
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{StatusCounts, TabSpec, TalkLine};

    fn text(state: &ChromeState, columns: u16, rows: u16) -> String {
        let area = Rect::new(0, 0, columns, rows);
        let mut buffer = Buffer::empty(area);
        draw(state, None, None, area, &mut buffer);
        (0..rows)
            .map(|y| {
                (0..columns)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn pane(id: &str, role: PaneRole, status: PaneStatus, focused: bool) -> PaneSpec {
        PaneSpec {
            id: AgentId::new(),
            name: format!(
                "claude{}",
                if id == "lead" {
                    String::new()
                } else {
                    format!(" {id}")
                }
            ),
            role,
            status,
            detail: String::new(),
            focused,
            unguarded: false,
        }
    }

    fn team() -> ChromeState {
        ChromeState {
            tabs: vec![
                TabSpec {
                    label: "auth-refactor".into(),
                    active: true,
                    attention: false,
                },
                TabSpec {
                    label: "fix-ci".into(),
                    active: false,
                    attention: true,
                },
            ],
            panes: vec![
                pane("lead", PaneRole::Lead, PaneStatus::Working, true),
                pane("2", PaneRole::Child(1), PaneStatus::NeedsInput, false),
                pane("3", PaneRole::Child(2), PaneStatus::Done, false),
            ],
            zoomed: None,
            mode: ChromeMode::Pane,
            talk: TalkLine {
                prompt: String::new(),
                text: String::new(),
                cursor: 0,
            },
            counts: StatusCounts {
                working: 1,
                waiting: 1,
                done: 1,
                failed: 0,
            },
            work_name: Some("auth-refactor".into()),
            clock: "14:02".into(),
            show_help: false,
            confirm: None,
            notice: None,
            key_style: KeyStyle::Helix,
            menu: None,
            bindings: Vec::new(),
            float: None,
            leader: "^Space".into(),
        }
    }

    #[test]
    fn team_layout() {
        insta::assert_snapshot!(text(&team(), 80, 16));
    }

    #[test]
    fn talking_to_everyone() {
        let mut state = team();
        state.mode = ChromeMode::Talk;
        state.talk = TalkLine {
            prompt: "@all ▸ ".into(),
            text: "summarise your changes".into(),
            cursor: 22,
        };
        let frame = render(&state, None, None, 80, 16);
        insta::assert_snapshot!(text(&state, 80, 16));
        assert!(
            frame.ansi.contains("\x1b[?25h"),
            "cursor shown while typing"
        );
    }

    #[test]
    fn empty_state_and_help() {
        let mut state = team();
        state.panes.clear();
        state.tabs.clear();
        state.work_name = None;
        state.mode = ChromeMode::Command;
        state.talk = TalkLine {
            prompt: "makima> ".into(),
            text: String::new(),
            cursor: 0,
        };
        insta::assert_snapshot!("empty", text(&state, 100, 30));
        insta::assert_snapshot!("empty_short", text(&state, 80, 16));
        state.notice = Some("unknown command \"launch\"".into());
        insta::assert_snapshot!("empty_with_notice", text(&state, 100, 30));
        state.notice = None;
        state.show_help = true;
        insta::assert_snapshot!("help", text(&state, 80, 24));
    }

    #[test]
    fn panes_sit_inside_their_borders() {
        let frame = render(&team(), None, None, 80, 16);
        assert_eq!(frame.panes.len(), 3);
        let lead = &frame.panes[0];
        assert_eq!((lead.x, lead.y), (1, 2));
        assert_eq!(lead.height, 16 - 3 - 2);
        assert_eq!(frame.tabs[1].index, 1);
        assert!(frame.tabs[0].end < frame.tabs[1].start);
    }

    #[test]
    fn zoom_shows_one_pane() {
        let mut state = team();
        let third = state.panes[2].id;
        state.zoomed = Some(third);
        let frame = render(&state, None, None, 80, 16);
        assert_eq!(
            frame.panes.iter().map(|p| p.id).collect::<Vec<_>>(),
            [third]
        );
        assert_eq!(frame.panes[0].width, 78);
    }

    #[test]
    fn unguarded_panes_are_flagged() {
        let mut state = team();
        state.panes[0].unguarded = true;
        let frame = render(&state, None, None, 100, 20);
        assert!(frame.ansi.contains("⚠ bypass"));
    }

    #[test]
    fn tools_float_over_quiet_panes() {
        let mut state = team();
        state.mode = ChromeMode::Tool;
        state.float = Some(FloatSpec::new("lazygit", "~/code/app", 90).with_hint("^Space hides"));
        let area = Rect::new(0, 0, 100, 30);
        let frame = render(&state, None, None, area.width, area.height);
        let Some(float) = frame.float else {
            panic!("a tool shows a float")
        };
        // 90% of the 100 × 27 body, centred, minus the border.
        assert_eq!(
            (float.x, float.y, float.width, float.height),
            (6, 3, 88, 22)
        );
        let shown = text(&state, 100, 30);
        assert!(shown.contains("lazygit · ~/code/app"), "{shown}");
        assert!(shown.contains("^Space hides"));
        assert!(shown.contains("TOOL"));

        state.float = state.float.map(|f| f.zoomed(true));
        let Some(zoomed) = render(&state, None, None, 100, 30).float else {
            panic!("a tool shows a float")
        };
        assert_eq!((zoomed.width, zoomed.height), (98, 25), "the whole body");
    }

    #[test]
    fn menus_come_out_as_an_overlay_above_the_panes() {
        let mut state = team();
        assert_eq!(
            render(&state, None, None, 100, 30).overlay,
            None,
            "nothing over the panes"
        );
        state.menu = Some(KeyMenu {
            title: "space".into(),
            entries: vec![
                ("v".into(), "speak".into()),
                ("x".into(), "close agent".into()),
            ],
        });
        let frame = render(&state, None, None, 100, 30);
        let Some(overlay) = frame.overlay else {
            panic!("a menu is drawn above the panes")
        };
        // The bottom-right infobox: 30 wide, 2 entries + border.
        assert_eq!(
            (
                overlay.rect.x,
                overlay.rect.y,
                overlay.rect.width,
                overlay.rect.height
            ),
            (69, 24, 30, 4)
        );
        assert!(overlay.ansi.contains("speak"));
        assert!(
            !overlay.ansi.contains("lead"),
            "only the overlay, not the panes"
        );
    }

    #[test]
    fn tiny_windows_render_a_notice() {
        let frame = render(&team(), None, None, 10, 3);
        assert!(frame.panes.is_empty());
    }
}
