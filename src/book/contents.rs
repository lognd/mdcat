// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The table of contents overlay of the book pager: every document in reading order, opened
//! on demand and closed again without a trace.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, StatefulWidget, Widget};

use super::Book;

/// Columns of indent per nesting level of a document.
const INDENT: usize = 2;

/// The key hint on the bottom border of the overlay.
const HINT: &str = " Enter open  Tab/Esc close ";

/// The open table of contents: which document is selected.
#[derive(Debug)]
pub(crate) struct Contents {
    count: usize,
    state: ListState,
    /// Rows of entries shown by the last render, for paging.
    view_height: u16,
}

impl Contents {
    /// Open the table of contents of `count` documents, selecting the `current` one.
    pub(crate) fn new(current: usize, count: usize) -> Self {
        Contents {
            count,
            state: ListState::default().with_selected(Some(current)),
            view_height: 0,
        }
    }

    /// The index of the selected document in reading order.
    pub(crate) fn selected(&self) -> usize {
        self.state.selected().unwrap_or(0)
    }

    /// Select the document `delta` entries away, stopping at the first and last.
    pub(crate) fn move_by(&mut self, delta: isize) {
        let last = self.count.saturating_sub(1);
        let selected = self.selected().saturating_add_signed(delta).min(last);
        self.state.select(Some(selected));
    }

    /// The number of entries a page up or down moves.
    pub(crate) fn page(&self) -> isize {
        isize::from(i16::try_from(self.view_height.saturating_sub(1).max(1)).unwrap_or(i16::MAX))
    }

    /// Draw the overlay centred in `area` over whatever is there, highlighting the selection and
    /// marking the `current` document.
    pub(crate) fn render(&mut self, book: &Book, current: usize, area: Rect, buf: &mut Buffer) {
        let items: Vec<ListItem<'_>> = book
            .chapters
            .iter()
            .enumerate()
            .map(|(index, chapter)| {
                let text = format!(" {}{} ", " ".repeat(chapter.depth * INDENT), chapter.title);
                let style = if index == current {
                    Style::new().add_modifier(Modifier::BOLD)
                } else {
                    Style::new()
                };
                ListItem::new(Line::styled(text, style))
            })
            .collect();
        let widest = items.iter().map(ListItem::width).max().unwrap_or(0);
        let popup = centred(area, widest.max(HINT.len()), items.len());
        self.view_height = popup.height.saturating_sub(2);

        let block = Block::bordered()
            .title(format!(
                " Contents {}/{} ",
                current + 1,
                book.chapters.len()
            ))
            .title_bottom(HINT);
        let list = List::new(items)
            .block(block)
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
        Clear.render(popup, buf);
        StatefulWidget::render(list, popup, buf, &mut self.state);
    }
}

/// A rectangle centred in `area` fitting `columns` x `rows` of content plus a border, never
/// larger than `area` less a cell of room around it.
fn centred(area: Rect, columns: usize, rows: usize) -> Rect {
    let fit = |content: usize, room: u16| {
        let wanted = u16::try_from(content.saturating_add(2)).unwrap_or(u16::MAX);
        wanted.min(room.saturating_sub(2).max(room.min(3)))
    };
    let width = fit(columns, area.width);
    let height = fit(rows, area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_starts_at_the_current_document_and_stops_at_the_ends() {
        let mut contents = Contents::new(2, 4);
        assert_eq!(contents.selected(), 2);
        contents.move_by(5);
        assert_eq!(contents.selected(), 3);
        contents.move_by(-10);
        assert_eq!(contents.selected(), 0);
    }

    #[test]
    fn overlay_is_centred_and_fits_the_area() {
        assert_eq!(
            centred(Rect::new(0, 0, 80, 24), 20, 4),
            Rect::new(29, 9, 22, 6)
        );
        assert_eq!(
            centred(Rect::new(0, 0, 80, 24), 200, 100),
            Rect::new(1, 1, 78, 22)
        );
        assert_eq!(centred(Rect::new(0, 0, 2, 2), 20, 4), Rect::new(0, 0, 2, 2));
    }
}
