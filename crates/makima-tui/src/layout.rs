//! Where panes go: the lead takes the left column and children stack on the
//! right (tmux's `main-vertical`); with many children the right column
//! becomes a two-column grid. Zoom gives one pane the whole body.

use makima_core::AgentId;
use ratatui::layout::{Constraint, Layout, Rect};

use crate::keys::Direction;
use crate::render::PaneRect;

/// Children stacked in one column before switching to a grid.
const MAX_STACKED_CHILDREN: usize = 4;
/// The lead's share of the width when there are children.
const LEAD_PERCENT: u16 = 55;

/// The outer rect (border included) of each of `count` panes, lead first.
pub(crate) fn pane_areas(body: Rect, count: usize) -> Vec<Rect> {
    match count {
        0 => Vec::new(),
        1 => vec![body],
        _ => {
            let [lead, rest] =
                Layout::horizontal([Constraint::Percentage(LEAD_PERCENT), Constraint::Fill(1)])
                    .areas(body);
            let children = count - 1;
            let mut areas = vec![lead];
            if children <= MAX_STACKED_CHILDREN {
                areas.extend(
                    Layout::vertical(vec![Constraint::Fill(1); children])
                        .split(rest)
                        .iter(),
                );
            } else {
                let rows = children.div_ceil(2);
                for (row, row_area) in Layout::vertical(vec![Constraint::Fill(1); rows])
                    .split(rest)
                    .iter()
                    .enumerate()
                {
                    let in_row = (children - row * 2).min(2);
                    areas.extend(
                        Layout::horizontal(vec![Constraint::Fill(1); in_row])
                            .split(*row_area)
                            .iter(),
                    );
                }
            }
            areas
        }
    }
}

/// The pane next to `from` in `direction`: the nearest one whose centre lies
/// that way, preferring panes that overlap on the other axis.
#[must_use]
pub fn neighbor(panes: &[PaneRect], from: AgentId, direction: Direction) -> Option<AgentId> {
    let origin = panes.iter().find(|p| p.id == from)?;
    let centre = |p: &PaneRect| {
        (
            i32::from(p.x) * 2 + i32::from(p.width),
            i32::from(p.y) * 2 + i32::from(p.height),
        )
    };
    let (ox, oy) = centre(origin);
    panes
        .iter()
        .filter(|p| p.id != from)
        .filter_map(|p| {
            let (x, y) = centre(p);
            let (along, across) = match direction {
                Direction::Left => (ox - x, (oy - y).abs()),
                Direction::Right => (x - ox, (oy - y).abs()),
                Direction::Up => (oy - y, (ox - x).abs()),
                Direction::Down => (y - oy, (ox - x).abs()),
            };
            (along > 0).then_some((across, along, p))
        })
        .min_by_key(|(across, along, _)| (*across, *along))
        .map(|(_, _, p)| p.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lead_left_children_stacked_right() {
        let areas = pane_areas(Rect::new(0, 0, 100, 40), 4);
        assert_eq!(areas[0], Rect::new(0, 0, 55, 40));
        assert!(areas[1..].iter().all(|a| a.x == 55 && a.width == 45));
        assert_eq!(areas[1..].iter().map(|a| a.height).sum::<u16>(), 40);
    }

    #[test]
    fn many_children_form_a_grid() {
        let areas = pane_areas(Rect::new(0, 0, 100, 40), 7);
        assert_eq!(areas.len(), 7);
        // Six children: three rows of two.
        assert_eq!(areas[1].y, areas[2].y);
        assert!(areas[1].x < areas[2].x);
    }

    #[test]
    fn neighbors_follow_the_layout() {
        let (lead, a, b) = (AgentId::new(), AgentId::new(), AgentId::new());
        let panes = [
            PaneRect::new(lead, 1, 1, 50, 38),
            PaneRect::new(a, 56, 1, 40, 18),
            PaneRect::new(b, 56, 21, 40, 18),
        ];
        assert_eq!(neighbor(&panes, lead, Direction::Right), Some(a));
        assert_eq!(neighbor(&panes, a, Direction::Down), Some(b));
        assert_eq!(neighbor(&panes, b, Direction::Left), Some(lead));
        assert_eq!(neighbor(&panes, lead, Direction::Left), None);
    }
}
