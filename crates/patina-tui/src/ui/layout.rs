use ratatui::layout::{Constraint, Layout, Rect};

pub struct FrameSections {
    pub header: Rect,
    pub sidebar: Rect,
    pub main: Rect,
    pub footer: Rect,
}

pub fn frame_sections(area: Rect) -> FrameSections {
    let vertical = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(0),
        Constraint::Length(3),
    ])
    .split(area);
    let horizontal =
        Layout::horizontal([Constraint::Length(34), Constraint::Min(0)]).split(vertical[1]);
    FrameSections {
        header: vertical[0],
        sidebar: horizontal[0],
        main: horizontal[1],
        footer: vertical[2],
    }
}
