// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The built-in pager of book mode: one document at a time, with Left and Right moving between
//! documents in reading order and a quiet status line at the bottom.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use mdcat::args::ImageProtocolChoice;
use pulldown_cmark_mdcat::ratatui::{
    detect_image_picker, ImageMode, ImagePicker, ImageProtocol, MdcatWidget, MdcatWidgetState,
    RenderOptions, Renderer,
};
use pulldown_cmark_mdcat::resources::ResourceUrlHandler;
use pulldown_cmark_mdcat::{expand_tabs, strip_frontmatter, Environment, Settings};
use ratatui::backend::Backend;
use ratatui::crossterm::event::{
    self as input, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::{Frame, Terminal};
use tracing::{event, Level};

use super::contents::Contents;
use super::Book;

/// The key hint at the right of the status line.
const KEY_HINT: &str = "<-/-> file  Tab contents  q quit";

/// Columns of left margin with `--margin`, matching mdcat's margin writer.
const MARGIN_COLUMNS: u16 = 2;

/// Whether and how the pager draws inline images.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PagerImages {
    /// Show images as alt text and links only.
    Off,
    /// Draw images if the terminal answers for kitty, iTerm2 or sixel graphics.
    Auto,
    /// Draw images with this protocol, as `--image-protocol` asked.
    Forced(ImageProtocol),
}

impl PagerImages {
    /// The image setting for `--image-protocol` (or its config default), if given.
    pub fn from_choice(choice: Option<ImageProtocolChoice>) -> Self {
        match choice {
            None => PagerImages::Auto,
            Some(ImageProtocolChoice::None) => PagerImages::Off,
            Some(ImageProtocolChoice::ITerm2) => PagerImages::Forced(ImageProtocol::Iterm2),
            Some(ImageProtocolChoice::Kitty) => PagerImages::Forced(ImageProtocol::Kitty),
            Some(ImageProtocolChoice::Sixel) => PagerImages::Forced(ImageProtocol::Sixel),
        }
    }
}

/// How the pager lays out and renders documents.
#[derive(Debug, Clone, Copy)]
pub struct PagerOptions {
    /// The widest column count to render a document at; a narrower terminal uses its width.
    pub max_columns: u16,
    /// Indent documents by two columns, like `--margin`.
    pub margin: bool,
    /// Expand tabs with this tab stop width before parsing, like `--tabs`.
    pub tabs: Option<u16>,
    /// Whether to draw inline images.
    pub images: PagerImages,
}

/// The widest column count to render at for `--columns` and `--full-width`, as in plain mdcat:
/// 80 by default, unbounded with `--full-width` or `--columns 0`.
pub fn max_columns(columns: Option<u16>, full_width: bool) -> u16 {
    match columns {
        None if full_width => u16::MAX,
        None => 80,
        Some(0) => u16::MAX,
        Some(columns) => columns,
    }
}

/// Something the reader asked for with a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    NextDocument,
    PreviousDocument,
    LineDown,
    LineUp,
    PageDown,
    PageUp,
    Top,
    Bottom,
    ToggleContents,
    CloseContents,
    Confirm,
    NextOrder,
    Quit,
}

/// The action bound to `key`, if any.
pub(crate) fn action_for(key: KeyEvent) -> Option<Action> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    let action = match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::Quit,
        KeyCode::Right | KeyCode::Char('l') => Action::NextDocument,
        KeyCode::Left | KeyCode::Char('h') => Action::PreviousDocument,
        KeyCode::Down => Action::LineDown,
        KeyCode::Up => Action::LineUp,
        KeyCode::PageDown | KeyCode::Char(' ') => Action::PageDown,
        KeyCode::PageUp => Action::PageUp,
        KeyCode::Char('g') => Action::Top,
        KeyCode::Char('G') => Action::Bottom,
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Tab => Action::ToggleContents,
        KeyCode::Esc => Action::CloseContents,
        KeyCode::Enter => Action::Confirm,
        KeyCode::Char('o') => Action::NextOrder,
        _ => return None,
    };
    Some(action)
}

/// A document's source, loaded the first time the reader opens it.
struct Loaded<'a> {
    markdown: String,
    renderer: Renderer<'a>,
}

/// One document of the book with its own scroll position and render cache, kept by path so
/// that it survives switching the book to another order.
#[derive(Default)]
struct Document<'a> {
    loaded: Option<Loaded<'a>>,
    state: MdcatWidgetState,
}

/// The pager over a book: which order and document are open and where each document is
/// scrolled to.
pub(crate) struct Pager<'a> {
    /// The book in each order the reader can switch to; `order` picks the open one.
    books: Vec<Book>,
    order: usize,
    /// Whether `books` holds every alternative order yet; they are resolved on first demand.
    alternatives_known: bool,
    render_options: RenderOptions<'a>,
    options: PagerOptions,
    image_picker: Option<ImagePicker>,
    documents: HashMap<PathBuf, Document<'a>>,
    /// The index of the open document in the open order.
    current: usize,
    /// The table of contents, while it is open.
    contents: Option<Contents>,
    /// Rows of document shown by the last draw, for paging and clamping.
    view_height: u16,
}

impl<'a> Pager<'a> {
    /// Create a pager on the first document of `book`, rendering like `settings`.
    pub(crate) fn new(
        book: Book,
        settings: &Settings<'a>,
        resource_handler: &'a dyn ResourceUrlHandler,
        options: PagerOptions,
        image_picker: Option<ImagePicker>,
    ) -> Self {
        let render_options = RenderOptions::default()
            .syntax_set(settings.syntax_set)
            .theme(settings.theme.clone())
            .syntax_theme(settings.syntax_theme.clone())
            .resource_handler(resource_handler)
            .images(match &image_picker {
                Some(picker) => ImageMode::Picker(picker.clone()),
                None => ImageMode::TextOnly,
            });
        Pager {
            books: vec![book],
            order: 0,
            alternatives_known: false,
            render_options,
            options,
            image_picker,
            documents: HashMap::new(),
            current: 0,
            contents: None,
            view_height: 0,
        }
    }

    /// The book in the open order.
    fn book(&self) -> &Book {
        &self.books[self.order]
    }

    /// The path of the open document.
    fn path(&self) -> &Path {
        &self.book().chapters[self.current].path
    }

    fn document(&self) -> Option<&Document<'a>> {
        self.documents.get(self.path())
    }

    fn document_mut(&mut self) -> &mut Document<'a> {
        let path = self.path().to_path_buf();
        self.documents.entry(path).or_default()
    }

    /// The scroll offset of the open document.
    pub(crate) fn scroll(&self) -> u16 {
        self.document()
            .map_or(0, |document| document.state.scroll())
    }

    /// Open the document at `index` in reading order, keeping every document's scroll position.
    pub(crate) fn open(&mut self, index: usize) {
        if index < self.book().chapters.len() && index != self.current {
            event!(
                target: "mdcat::book",
                Level::DEBUG,
                from = self.current,
                to = index,
                "Opening {}",
                self.book().chapters[index].path.display()
            );
            self.current = index;
        }
    }

    /// Switch the book to its next order that applies, staying on the open document.
    fn next_order(&mut self) {
        if !self.alternatives_known {
            self.books = self.book().alternatives();
            self.order = 0;
            self.alternatives_known = true;
        }
        if self.books.len() < 2 {
            event!(target: "mdcat::book", Level::DEBUG, "No other order applies");
            return;
        }
        let path = self.path().to_path_buf();
        self.order = (self.order + 1) % self.books.len();
        self.current = self
            .book()
            .chapters
            .iter()
            .position(|chapter| chapter.path == path)
            .unwrap_or(0);
        event!(
            target: "mdcat::book",
            Level::DEBUG,
            order = ?self.book().order,
            current = self.current,
            "Switched order to {}",
            self.book().order_label()
        );
    }

    /// Apply `action` to the table of contents if it is open, else to the open document;
    /// returns `false` once the reader asked to quit.
    pub(crate) fn apply(&mut self, action: Action) -> bool {
        match self.contents.as_mut() {
            Some(contents) => {
                match action {
                    Action::LineDown => contents.move_by(1),
                    Action::LineUp => contents.move_by(-1),
                    Action::PageDown => contents.move_by(contents.page()),
                    Action::PageUp => contents.move_by(-contents.page()),
                    Action::Top => contents.move_by(isize::MIN),
                    Action::Bottom => contents.move_by(isize::MAX),
                    Action::Confirm => {
                        let selected = contents.selected();
                        self.close_contents();
                        self.open(selected);
                    }
                    Action::ToggleContents | Action::CloseContents => self.close_contents(),
                    Action::Quit => return self.apply_to_document(action),
                    Action::NextDocument | Action::PreviousDocument | Action::NextOrder => {}
                }
                true
            }
            None => self.apply_to_document(action),
        }
    }

    fn close_contents(&mut self) {
        event!(target: "mdcat::book", Level::DEBUG, "Closing the table of contents");
        self.contents = None;
    }

    fn apply_to_document(&mut self, action: Action) -> bool {
        let page = self.view_height.saturating_sub(1).max(1);
        match action {
            Action::NextDocument => self.open(self.current + 1),
            Action::PreviousDocument => self.open(self.current.saturating_sub(1)),
            Action::LineDown => self.scroll_to(self.scroll().saturating_add(1)),
            Action::LineUp => self.scroll_to(self.scroll().saturating_sub(1)),
            Action::PageDown => self.scroll_to(self.scroll().saturating_add(page)),
            Action::PageUp => self.scroll_to(self.scroll().saturating_sub(page)),
            Action::Top => self.scroll_to(0),
            Action::Bottom => self.scroll_to(u16::MAX),
            Action::ToggleContents => {
                event!(target: "mdcat::book", Level::DEBUG, "Opening the table of contents");
                self.contents = Some(Contents::new(self.current, self.book()));
            }
            Action::NextOrder => self.next_order(),
            Action::CloseContents | Action::Confirm => {}
            Action::Quit => {
                event!(target: "mdcat::book", Level::DEBUG, "Quitting the pager");
                return false;
            }
        }
        true
    }

    /// Scroll the open document to `scroll`, clamped so that its last line stays at the bottom.
    fn scroll_to(&mut self, scroll: u16) {
        let max = self.max_scroll();
        self.document_mut().state.set_scroll(scroll.min(max));
    }

    /// The largest useful scroll offset of the open document, as of the last draw.
    fn max_scroll(&self) -> u16 {
        let lines = self
            .document()
            .map_or(0, |document| document.state.total_lines());
        u16::try_from(lines)
            .unwrap_or(u16::MAX)
            .saturating_sub(self.view_height)
    }

    /// How far down the open document the view is: a percent, or `(END)` once its last line is
    /// visible, followed by the title of the document Right opens next, if any.
    fn progress(&self) -> String {
        let max = self.max_scroll();
        if self.scroll() < max {
            let percent = u32::from(self.scroll()) * 100 / u32::from(max);
            return format!("{percent}%");
        }
        match self.book().chapters.get(self.current + 1) {
            Some(next) => format!("(END) -> {}", next.title),
            None => "(END)".to_string(),
        }
    }

    /// Read the open document unless it was read already.
    ///
    /// A document that cannot be read shows the error in its place, so the rest of the book
    /// stays readable.
    fn load(&mut self) {
        if self
            .document()
            .is_some_and(|document| document.loaded.is_some())
        {
            return;
        }
        let path = self.path().to_path_buf();
        let source = mdcat::read_input(path.to_string_lossy()).and_then(|(base_dir, input)| {
            let input = strip_frontmatter(&input);
            let markdown = match self.options.tabs {
                Some(tab_width) => expand_tabs(input, tab_width).into_owned(),
                None => input.to_string(),
            };
            Ok((Environment::for_local_directory(&base_dir)?, markdown))
        });
        let (environment, markdown) = match source {
            Ok(source) => source,
            Err(error) => {
                event!(
                    target: "mdcat::book",
                    Level::ERROR,
                    "Failed to read {}: {error:#}",
                    path.display()
                );
                let markdown = format!("**Error:** {}: {error:#}", path.display());
                let environment =
                    std::env::current_dir().and_then(|cwd| Environment::for_local_directory(&cwd));
                match environment {
                    Ok(environment) => (environment, markdown),
                    Err(error) => {
                        event!(target: "mdcat::book", Level::ERROR, %error, "No base directory");
                        return;
                    }
                }
            }
        };
        let renderer = Renderer::new(self.render_options.clone().environment(environment));
        let image_picker = self.image_picker.clone();
        let document = self.document_mut();
        if let Some(picker) = image_picker {
            document.state.set_image_picker(picker);
        }
        document.loaded = Some(Loaded { markdown, renderer });
    }

    /// The area to render the open document into: below the margin, at most `max_columns` wide.
    fn content_area(&self, body: Rect) -> Rect {
        let margin = if self.options.margin {
            MARGIN_COLUMNS.min(body.width)
        } else {
            0
        };
        Rect {
            x: body.x + margin,
            width: (body.width - margin).min(self.options.max_columns),
            ..body
        }
    }

    /// Draw the open document and the status line.
    pub(crate) fn draw(&mut self, frame: &mut Frame<'_>) {
        let [body, status] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(frame.area());
        let content = self.content_area(body);
        self.view_height = content.height;
        self.load();
        self.render_document(frame, content);
        // A resize or a re-render at a new width can leave the scroll past the end.
        let max = self.max_scroll();
        if self.scroll() > max {
            self.scroll_to(max);
            self.render_document(frame, content);
        }
        frame.render_widget(self.status_line(status.width), status);
        if let Some(contents) = self.contents.as_mut() {
            contents.render(
                &self.books[self.order],
                self.current,
                body,
                frame.buffer_mut(),
            );
        }
    }

    fn render_document(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let path = self.path().to_path_buf();
        let Some(document) = self.documents.get_mut(&path) else {
            return;
        };
        if let Some(loaded) = &document.loaded {
            let widget =
                MdcatWidget::with_renderer(loaded.markdown.as_str(), loaded.renderer.clone());
            frame.render_stateful_widget(widget, area, &mut document.state);
        }
    }

    /// The status line: title, position, order source and progress, then the key hint.
    fn status_line(&self, width: u16) -> Line<'static> {
        let book = self.book();
        let chapter = &book.chapters[self.current];
        let left = format!(
            " {}   {}/{}   {}   {}",
            chapter.title,
            self.current + 1,
            book.chapters.len(),
            book.order_label(),
            self.progress()
        );
        let used = Line::from(left.as_str()).width() + KEY_HINT.len() + 1;
        let text = match usize::from(width).checked_sub(used) {
            Some(gap) if gap >= 2 => format!("{left}{}{KEY_HINT} ", " ".repeat(gap)),
            _ => left,
        };
        Line::styled(text, Style::new().add_modifier(Modifier::DIM))
    }

    /// Draw and handle keys until the reader quits; resizes redraw at the new size.
    fn event_loop<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()>
    where
        B::Error: Send + Sync + 'static,
    {
        loop {
            terminal
                .draw(|frame| self.draw(frame))
                .context("Failed to draw the pager")?;
            match input::read().context("Failed to read terminal input")? {
                Event::Key(key) => {
                    if let Some(action) = action_for(key) {
                        if !self.apply(action) {
                            return Ok(());
                        }
                    }
                }
                Event::Resize(columns, rows) => {
                    event!(target: "mdcat::book", Level::TRACE, columns, rows, "Resized");
                }
                _ => {}
            }
        }
    }
}

/// Find an image protocol for `images`, probing the terminal; `None` means text only.
///
/// Must run after the terminal entered raw mode, since it reads the terminal's answers.
fn image_picker(images: PagerImages) -> Option<ImagePicker> {
    if images == PagerImages::Off {
        return None;
    }
    let mut picker = match detect_image_picker() {
        Ok(picker) => picker,
        Err(error) => {
            event!(target: "mdcat::book", Level::WARN, %error, "Image detection failed");
            return None;
        }
    };
    match images {
        PagerImages::Forced(protocol) => picker.set_protocol_type(protocol),
        PagerImages::Auto if picker.protocol_type() == ImageProtocol::Halfblocks => {
            event!(target: "mdcat::book", Level::DEBUG, "No graphics protocol, text only");
            return None;
        }
        PagerImages::Auto | PagerImages::Off => {}
    }
    event!(
        target: "mdcat::book",
        Level::DEBUG,
        protocol = ?picker.protocol_type(),
        "Drawing images"
    );
    Some(picker)
}

/// Page through `book` on the terminal until the reader quits, restoring the terminal after.
pub fn run(
    book: Book,
    settings: &Settings<'_>,
    resource_handler: &dyn ResourceUrlHandler,
    options: PagerOptions,
) -> Result<()> {
    event!(
        target: "mdcat::book",
        Level::DEBUG,
        ?options,
        documents = book.chapters.len(),
        "Starting the pager"
    );
    let mut terminal = ratatui::try_init().context("Failed to set up the terminal")?;
    let picker = image_picker(options.images);
    let result =
        Pager::new(book, settings, resource_handler, options, picker).event_loop(&mut terminal);
    let restored = ratatui::try_restore();
    result?;
    restored.map_err(|error: io::Error| anyhow::anyhow!("Failed to restore the terminal: {error}"))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use pulldown_cmark_mdcat::resources::NoopResourceHandler;
    use pulldown_cmark_mdcat::terminal::{TerminalProgram, TerminalSize};
    use pulldown_cmark_mdcat::Theme;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use syntect::parsing::SyntaxSet;

    use mdcat::args::BookOrder;

    use super::*;

    fn book() -> Book {
        fixture_book("long")
    }

    fn fixture_book(name: &str) -> Book {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/book")
            .join(name);
        Book::resolve(&root, BookOrder::Auto).unwrap()
    }

    fn options() -> PagerOptions {
        PagerOptions {
            max_columns: 80,
            margin: false,
            tabs: None,
            images: PagerImages::Off,
        }
    }

    fn settings(syntax_set: &SyntaxSet) -> Settings<'_> {
        Settings {
            terminal_capabilities: TerminalProgram::Ansi.capabilities(),
            terminal_size: TerminalSize::default(),
            syntax_set,
            theme: Theme::dark(),
            syntax_theme: None,
        }
    }

    fn row(buffer: &Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
    }

    /// The position of the first cell of `needle` inside the bordered contents overlay.
    fn find(buffer: &Buffer, needle: &str) -> (u16, u16) {
        (0..buffer.area.height)
            .find_map(|y| {
                let row = row(buffer, y);
                let inside = row.split('\u{2502}').nth(1)?;
                let start = row.find(inside)?;
                inside.find(needle).map(|offset| {
                    let byte = start + offset;
                    let x = row[..byte].chars().count();
                    (u16::try_from(x).unwrap(), y)
                })
            })
            .unwrap_or_else(|| panic!("{needle:?} not on screen:\n{}", screen(buffer)))
    }

    fn screen(buffer: &Buffer) -> String {
        (0..buffer.area.height)
            .map(|y| row(buffer, y))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Draw `pager` on a terminal of `width` x `height` and return what is on screen.
    fn draw(pager: &mut Pager<'_>, width: u16, height: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| pager.draw(frame)).unwrap();
        terminal.backend().buffer().clone()
    }

    fn with_pager(test: impl FnOnce(&mut Pager<'_>)) {
        let book = book();
        let syntax_set = two_face::syntax::extra_newlines();
        let settings = settings(&syntax_set);
        let mut pager = Pager::new(book, &settings, &NoopResourceHandler, options(), None);
        test(&mut pager);
    }

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn keys_map_to_actions() {
        let cases = [
            (KeyCode::Right, Action::NextDocument),
            (KeyCode::Char('l'), Action::NextDocument),
            (KeyCode::Left, Action::PreviousDocument),
            (KeyCode::Char('h'), Action::PreviousDocument),
            (KeyCode::Down, Action::LineDown),
            (KeyCode::Up, Action::LineUp),
            (KeyCode::PageDown, Action::PageDown),
            (KeyCode::Char(' '), Action::PageDown),
            (KeyCode::PageUp, Action::PageUp),
            (KeyCode::Char('g'), Action::Top),
            (KeyCode::Char('G'), Action::Bottom),
            (KeyCode::Char('q'), Action::Quit),
            (KeyCode::Tab, Action::ToggleContents),
            (KeyCode::Esc, Action::CloseContents),
            (KeyCode::Enter, Action::Confirm),
            (KeyCode::Char('o'), Action::NextOrder),
        ];
        for (code, action) in cases {
            assert_eq!(action_for(press(code)), Some(action), "{code:?}");
        }
        assert_eq!(
            action_for(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(Action::Quit)
        );
        assert_eq!(action_for(press(KeyCode::Char('x'))), None);
    }

    #[test]
    fn left_and_right_move_between_documents_and_stop_at_the_ends() {
        with_pager(|pager| {
            draw(pager, 60, 10);
            assert!(pager.apply(Action::PreviousDocument));
            assert_eq!(pager.current, 0);
            pager.apply(Action::NextDocument);
            assert_eq!(pager.current, 1);
            assert!(screen(&draw(pager, 60, 10)).contains("Second chapter"));
            pager.apply(Action::NextDocument);
            pager.apply(Action::NextDocument);
            assert_eq!(pager.current, 1);
            pager.apply(Action::PreviousDocument);
            assert_eq!(pager.current, 0);
            assert!(!pager.apply(Action::Quit));
        });
    }

    #[test]
    fn scrolling_is_clamped_to_the_document() {
        with_pager(|pager| {
            draw(pager, 60, 10);
            let view = 9;
            pager.apply(Action::LineDown);
            assert_eq!(pager.scroll(), 1);
            pager.apply(Action::LineUp);
            pager.apply(Action::LineUp);
            assert_eq!(pager.scroll(), 0);
            pager.apply(Action::PageDown);
            assert_eq!(pager.scroll(), view - 1);
            pager.apply(Action::PageUp);
            assert_eq!(pager.scroll(), 0);
            pager.apply(Action::Bottom);
            let bottom = pager.scroll();
            assert!(bottom > view, "the fixture is longer than two pages");
            pager.apply(Action::LineDown);
            pager.apply(Action::PageDown);
            assert_eq!(pager.scroll(), bottom);
            let screen = screen(&draw(pager, 60, 10));
            assert!(
                screen.contains("Last line of the first chapter"),
                "{screen}"
            );
            pager.apply(Action::Top);
            assert_eq!(pager.scroll(), 0);
        });
    }

    #[test]
    fn each_document_keeps_its_scroll_position() {
        with_pager(|pager| {
            draw(pager, 60, 10);
            pager.apply(Action::PageDown);
            let first = pager.scroll();
            assert!(first > 0);
            pager.apply(Action::NextDocument);
            draw(pager, 60, 10);
            assert_eq!(pager.scroll(), 0);
            pager.apply(Action::LineDown);
            pager.apply(Action::PreviousDocument);
            assert_eq!(pager.scroll(), first);
            pager.apply(Action::NextDocument);
            assert_eq!(pager.scroll(), 1);
        });
    }

    #[test]
    fn status_line_shows_title_position_order_percent_and_hint() {
        with_pager(|pager| {
            let buffer = draw(pager, 80, 10);
            let status = row(&buffer, 9);
            assert!(
                status.starts_with(" First chapter   1/2   README + by path   0%"),
                "{status:?}"
            );
            assert!(
                status.ends_with("<-/-> file  Tab contents  q quit "),
                "{status:?}"
            );
        });
    }

    #[test]
    fn end_of_a_document_shows_end_and_the_next_title() {
        with_pager(|pager| {
            draw(pager, 80, 10);
            pager.apply(Action::PageDown);
            let status = row(&draw(pager, 80, 10), 9);
            assert!(
                status.contains("   1/2   README + by path   "),
                "{status:?}"
            );
            assert!(!status.contains("(END)"), "{status:?}");
            pager.apply(Action::Bottom);
            let status = row(&draw(pager, 80, 10), 9);
            assert!(
                status.contains("README + by path   (END) -> Second chapter"),
                "{status:?}"
            );
        });
    }

    #[test]
    fn end_of_the_last_document_has_no_next_title() {
        with_pager(|pager| {
            pager.apply(Action::NextDocument);
            draw(pager, 80, 10);
            pager.apply(Action::Bottom);
            let status = row(&draw(pager, 80, 10), 9);
            assert!(
                status.contains("2/2   README + by path   (END) "),
                "{status:?}"
            );
            assert!(!status.contains("->  "), "{status:?}");
            assert!(!status.contains("(END) ->"), "{status:?}");
        });
    }

    #[test]
    fn narrow_status_line_drops_the_hint() {
        with_pager(|pager| {
            let status = row(&draw(pager, 30, 10), 9);
            assert!(status.starts_with(" First chapter   1/2"), "{status:?}");
            assert!(!status.contains("quit"), "{status:?}");
        });
    }

    #[test]
    fn resizing_re_renders_at_the_new_width_and_clamps_the_scroll() {
        with_pager(|pager| {
            let wide = screen(&draw(pager, 80, 10));
            let narrow = screen(&draw(pager, 30, 10));
            assert_ne!(wide, narrow);
            pager.apply(Action::Bottom);
            let bottom_narrow = pager.scroll();
            draw(pager, 80, 40);
            assert!(pager.scroll() < bottom_narrow);
        });
    }

    #[test]
    fn content_is_capped_and_indented() {
        let book = book();
        let syntax_set = two_face::syntax::extra_newlines();
        let settings = settings(&syntax_set);
        let options = PagerOptions {
            max_columns: 20,
            margin: true,
            ..options()
        };
        let pager = Pager::new(book, &settings, &NoopResourceHandler, options, None);
        let area = pager.content_area(Rect::new(0, 0, 100, 9));
        assert_eq!(area, Rect::new(2, 0, 20, 9));
        let area = pager.content_area(Rect::new(0, 0, 1, 9));
        assert_eq!(area, Rect::new(1, 0, 0, 9));
    }

    #[test]
    fn contents_are_closed_until_tab_opens_them() {
        with_pager(|pager| {
            let closed = screen(&draw(pager, 60, 10));
            assert!(!pager.contents.is_some());
            assert!(!closed.contains("Contents"), "{closed}");
            pager.apply(Action::ToggleContents);
            let open = screen(&draw(pager, 60, 10));
            assert!(open.contains("Contents 1/2"), "{open}");
            assert!(open.contains("First chapter"), "{open}");
            assert!(open.contains("Second chapter"), "{open}");
            assert!(open.contains("Enter open"), "{open}");
        });
    }

    #[test]
    fn contents_highlight_the_current_document() {
        with_pager(|pager| {
            pager.apply(Action::NextDocument);
            pager.apply(Action::ToggleContents);
            let buffer = draw(pager, 60, 10);
            let (x, y) = find(&buffer, "Second chapter");
            let cell = &buffer[(x, y)];
            assert!(cell.modifier.contains(Modifier::REVERSED), "selected");
            assert!(cell.modifier.contains(Modifier::BOLD), "current");
            let (x, y) = find(&buffer, " First chapter ");
            assert!(!buffer[(x + 1, y)].modifier.contains(Modifier::REVERSED));
        });
    }

    #[test]
    fn enter_jumps_to_the_selected_document_and_closes_the_contents() {
        with_pager(|pager| {
            draw(pager, 60, 10);
            pager.apply(Action::PageDown);
            let scroll = pager.scroll();
            pager.apply(Action::ToggleContents);
            pager.apply(Action::LineDown);
            // Left and Right do nothing while the contents are open.
            pager.apply(Action::PreviousDocument);
            pager.apply(Action::Confirm);
            assert!(!pager.contents.is_some());
            assert_eq!(pager.current, 1);
            let shown = screen(&draw(pager, 60, 10));
            assert!(!shown.contains("Contents"), "{shown}");
            pager.apply(Action::ToggleContents);
            pager.apply(Action::Top);
            pager.apply(Action::Confirm);
            assert_eq!(pager.current, 0);
            assert_eq!(pager.scroll(), scroll);
        });
    }

    #[test]
    fn tab_or_esc_close_the_contents_without_moving() {
        for close in [Action::ToggleContents, Action::CloseContents] {
            with_pager(|pager| {
                draw(pager, 60, 10);
                pager.apply(Action::ToggleContents);
                pager.apply(Action::Bottom);
                pager.apply(close);
                assert!(!pager.contents.is_some());
                assert_eq!(pager.current, 0);
                assert_eq!(pager.scroll(), 0);
                let shown = screen(&draw(pager, 60, 10));
                assert!(!shown.contains("Contents"), "{shown}");
            });
        }
    }

    #[test]
    fn o_switches_to_the_next_order_keeping_the_document_and_its_scroll() {
        let book = fixture_book("index-table");
        let syntax_set = two_face::syntax::extra_newlines();
        let settings = settings(&syntax_set);
        let mut pager = Pager::new(book, &settings, &NoopResourceHandler, options(), None);
        pager.apply(Action::NextDocument);
        draw(&mut pager, 30, 4);
        pager.apply(Action::LineDown);
        assert_eq!(pager.path().file_name().unwrap(), "goals.md");
        let status = row(&draw(&mut pager, 120, 4), 3);
        assert!(
            status.contains("2/10   README.md, by path +3 unlisted"),
            "{status:?}"
        );

        pager.apply(Action::NextOrder);
        assert_eq!(pager.path().file_name().unwrap(), "goals.md");
        assert_eq!(pager.current, 4);
        let status = row(&draw(&mut pager, 120, 4), 3);
        assert!(status.contains("5/10   README + by path"), "{status:?}");

        pager.apply(Action::NextOrder);
        assert_eq!(pager.current, 1);
        assert_eq!(pager.book().order, BookOrder::Auto);
        draw(&mut pager, 30, 4);
        assert_eq!(pager.scroll(), 1);
    }

    #[test]
    fn o_does_nothing_with_one_order_or_with_the_contents_open() {
        with_pager(|pager| {
            pager.apply(Action::NextDocument);
            pager.apply(Action::NextOrder);
            assert_eq!(pager.current, 1);
            assert_eq!(pager.books.len(), 1);
            pager.apply(Action::ToggleContents);
            pager.apply(Action::NextOrder);
            assert!(pager.contents.is_some());
            assert_eq!(pager.current, 1);
        });
    }

    #[test]
    fn quit_works_with_the_contents_open() {
        with_pager(|pager| {
            pager.apply(Action::ToggleContents);
            assert!(!pager.apply(Action::Quit));
        });
    }

    #[test]
    fn image_protocol_choice_maps_to_pager_images() {
        assert_eq!(PagerImages::from_choice(None), PagerImages::Auto);
        assert_eq!(
            PagerImages::from_choice(Some(ImageProtocolChoice::None)),
            PagerImages::Off
        );
        assert_eq!(
            PagerImages::from_choice(Some(ImageProtocolChoice::Kitty)),
            PagerImages::Forced(ImageProtocol::Kitty)
        );
    }

    #[test]
    fn max_columns_follows_columns_and_full_width() {
        assert_eq!(max_columns(None, false), 80);
        assert_eq!(max_columns(None, true), u16::MAX);
        assert_eq!(max_columns(Some(0), false), u16::MAX);
        assert_eq!(max_columns(Some(100), false), 100);
    }
}
