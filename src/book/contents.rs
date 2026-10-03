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

/// The heading above documents an index document left out.
const UNLISTED_HEADING: &str = "Unlisted";

/// A row of the table of contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Row {
    /// The heading above a run of unlisted documents, at this depth; never selected.
    Unlisted(usize),
    /// The document at this index in reading order.
    Chapter(usize),
}

/// The open table of contents: its rows and which document is selected.
#[derive(Debug)]
pub(crate) struct Contents {
    rows: Vec<Row>,
    /// The indices in `rows` of the rows that are documents, in order.
    chapter_rows: Vec<usize>,
    state: ListState,
    /// Rows of entries shown by the last render, for paging.
    view_height: u16,
}

impl Contents {
    /// Open the table of contents of `book`, selecting the `current` document.
    pub(crate) fn new(current: usize, book: &Book) -> Self {
        let mut rows = Vec::new();
        for (index, chapter) in book.chapters.iter().enumerate() {
            let starts_run = index == 0 || !book.chapters[index - 1].unlisted;
            if chapter.unlisted && starts_run {
                rows.push(Row::Unlisted(chapter.depth));
            }
            rows.push(Row::Chapter(index));
        }
        let chapter_rows: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| matches!(row, Row::Chapter(_)))
            .map(|(position, _)| position)
            .collect();
        let selected = chapter_rows.get(current).copied();
        Contents {
            rows,
            chapter_rows,
            state: ListState::default().with_selected(selected),
            view_height: 0,
        }
    }

    /// The index of the selected document in reading order.
    pub(crate) fn selected(&self) -> usize {
        match self.state.selected().and_then(|row| self.rows.get(row)) {
            Some(Row::Chapter(index)) => *index,
            Some(Row::Unlisted(_)) | None => 0,
        }
    }

    /// Select the document `delta` documents away, stopping at the first and last and skipping
    /// headings.
    pub(crate) fn move_by(&mut self, delta: isize) {
        let position = self
            .chapter_rows
            .iter()
            .position(|row| Some(*row) == self.state.selected())
            .unwrap_or(0);
        let last = self.chapter_rows.len().saturating_sub(1);
        let position = position.saturating_add_signed(delta).min(last);
        self.state.select(self.chapter_rows.get(position).copied());
    }

    /// The number of entries a page up or down moves.
    pub(crate) fn page(&self) -> isize {
        isize::from(i16::try_from(self.view_height.saturating_sub(1).max(1)).unwrap_or(i16::MAX))
    }

    /// Draw the overlay centred in `area` over whatever is there, highlighting the selection and
    /// marking the `current` document.
    pub(crate) fn render(&mut self, book: &Book, current: usize, area: Rect, buf: &mut Buffer) {
        let items: Vec<ListItem<'_>> = self
            .rows
            .iter()
            .map(|row| {
                let (depth, title, style) = match *row {
                    Row::Unlisted(depth) => (
                        depth,
                        UNLISTED_HEADING,
                        Style::new().add_modifier(Modifier::DIM | Modifier::ITALIC),
                    ),
                    Row::Chapter(index) => {
                        let chapter = &book.chapters[index];
                        let style = if index == current {
                            Style::new().add_modifier(Modifier::BOLD)
                        } else {
                            Style::new()
                        };
                        (chapter.depth, chapter.title.as_str(), style)
                    }
                };
                let text = format!(" {}{title} ", " ".repeat(depth * INDENT));
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

    use std::path::PathBuf;

    use mdcat::args::BookOrder;

    use crate::book::order::{Chapter, OrderSource};

    /// A book of `unlisted.len()` documents, the ones flagged true unlisted.
    fn book(unlisted: &[bool]) -> Book {
        Book {
            target: PathBuf::from("book"),
            order: BookOrder::Auto,
            chapters: unlisted
                .iter()
                .enumerate()
                .map(|(index, unlisted)| Chapter {
                    path: PathBuf::from(format!("{index}.md")),
                    title: format!("Doc {index}"),
                    depth: 0,
                    unlisted: *unlisted,
                })
                .collect(),
            sources: vec![OrderSource::Path],
        }
    }

    #[test]
    fn selection_starts_at_the_current_document_and_stops_at_the_ends() {
        let mut contents = Contents::new(2, &book(&[false; 4]));
        assert_eq!(contents.selected(), 2);
        contents.move_by(5);
        assert_eq!(contents.selected(), 3);
        contents.move_by(-10);
        assert_eq!(contents.selected(), 0);
    }

    #[test]
    fn unlisted_runs_get_a_heading_the_selection_skips() {
        let book = book(&[false, false, true, true]);
        let mut contents = Contents::new(1, &book);
        assert_eq!(
            contents.rows,
            vec![
                Row::Chapter(0),
                Row::Chapter(1),
                Row::Unlisted(0),
                Row::Chapter(2),
                Row::Chapter(3)
            ]
        );
        contents.move_by(1);
        assert_eq!(contents.selected(), 2);
        contents.move_by(-1);
        assert_eq!(contents.selected(), 1);
        contents.move_by(isize::MAX);
        assert_eq!(contents.selected(), 3);

        let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 12));
        contents.render(&book, 1, buffer.area, &mut buffer);
        let screen: String = (0..12)
            .map(|y| (0..40).map(|x| buffer[(x, y)].symbol()).collect::<String>() + "\n")
            .collect();
        let doc = screen.find("Doc 1").unwrap();
        let heading = screen.find("Unlisted").unwrap();
        assert!(
            doc < heading && heading < screen.find("Doc 2").unwrap(),
            "{screen}"
        );
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
