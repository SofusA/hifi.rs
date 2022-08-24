mod player;

use crate::{
    qobuz::track::PlaylistTrack,
    state::{
        app::{AppState, PlayerKey, StateKey},
        ClockValue, FloatValue, StatusValue,
    },
};
use textwrap::fill;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::bar,
    text::{Span, Spans, Text},
    widgets::{
        Block, BorderType, Borders, Clear, List as TermList, ListItem, ListState, Paragraph,
        Row as TermRow, Table as TermTable, TableState, Tabs,
    },
    Frame,
};

pub fn player<B>(f: &mut Frame<B>, rect: Rect, state: AppState)
where
    B: Backend,
{
    let tree = state.player;
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Max(5), Constraint::Length(1)])
        .margin(0)
        .split(rect);

    if let Some(track) = get_player!(PlayerKey::NextUp, tree, PlaylistTrack) {
        if let Some(status) = get_player!(PlayerKey::Status, tree, StatusValue) {
            player::current_track(track, status, f, layout[0]);
        }
    }

    if let (Some(position), Some(duration), Some(prog)) = (
        get_player!(PlayerKey::Position, tree, ClockValue),
        get_player!(PlayerKey::Duration, tree, ClockValue),
        get_player!(PlayerKey::Progress, tree, FloatValue),
    ) {
        player::progress(position, duration, prog, f, layout[1]);
    } else {
        f.render_widget(
            Block::default().style(Style::default().bg(Color::Indexed(236))),
            layout[1],
        )
    }
}

pub fn text_box<B>(f: &mut Frame<B>, text: String, title: Option<&str>, area: Rect)
where
    B: Backend,
{
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(Color::Indexed(250)));

    if let Some(title) = title {
        block = block.title(title);
    }

    let p = Paragraph::new(text).block(block);

    f.render_widget(p, area);
}

pub fn list<'t, B>(f: &mut Frame<B>, list: &'t mut List<'_>, area: Rect)
where
    B: Backend,
{
    let layout = Layout::default()
        .margin(0)
        .constraints([Constraint::Min(1)])
        .split(area);

    let term_list = TermList::new(list.list_items())
        .highlight_style(
            Style::default()
                .fg(Color::Indexed(81))
                .bg(Color::Indexed(235)),
        )
        .highlight_symbol("");

    f.render_stateful_widget(term_list, layout[0], &mut list.state);
}

pub fn table<'r, B>(f: &mut Frame<B>, table: &'r mut Table, area: Rect)
where
    B: Backend,
{
    let (rows, widths, header) = table.term_table(f.size().width);

    let term_table = TermTable::new(rows)
        .header(
            TermRow::new(header).style(
                Style::default()
                    .bg(Color::Indexed(236))
                    .fg(Color::Indexed(81)),
            ),
        )
        .block(Block::default())
        .widths(widths.as_slice())
        .style(Style::default().fg(Color::Indexed(250)))
        .highlight_style(
            Style::default()
                .fg(Color::Indexed(81))
                .bg(Color::Indexed(235)),
        );

    f.render_stateful_widget(term_table, area, &mut table.state.clone());
}

pub fn tabs<B>(num: usize, f: &mut Frame<B>, rect: Rect)
where
    B: Backend,
{
    let padding = (rect.width as usize / 2) - 4;

    let titles = ["Now Playing", "Search Results"]
        .iter()
        .cloned()
        .map(|t| {
            let text = format!("{:^padding$}", t);
            Spans::from(text)
        })
        .collect();

    let mut bar = Span::from(bar::FULL);
    bar.style = Style::default().fg(Color::Indexed(236));

    let tabs = Tabs::new(titles)
        .block(Block::default().style(Style::default().bg(Color::Indexed(235))))
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .bg(Color::Indexed(81))
                .fg(Color::Indexed(235))
                .add_modifier(Modifier::BOLD),
        )
        .divider(bar)
        .select(num);

    f.render_widget(tabs, rect);
}
#[allow(unused)]
fn search_popup<B>(f: &mut Frame<B>, search_query: Vec<char>)
where
    B: Backend,
{
    let block = Block::default()
        .title("Enter query")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Indexed(250)));

    let p = Paragraph::new(Text::from(Spans::from(
        search_query
            .iter()
            .map(|c| Span::from(c.to_string()))
            .collect::<Vec<Span>>(),
    )))
    .block(block);

    let area = centered_rect(60, 10, f.size());

    f.render_widget(Clear, area);
    f.render_widget(p, area);
    f.set_cursor(area.x + 1 + search_query.len() as u16, area.y + 1);
}

/// helper function to create a centered rect using up certain percentage of the available rect `r`
/// https://github.com/fdehau/tui-rs/blob/master/examples/popup.rs
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}

#[derive(Clone, Debug)]
pub struct Item<'i>(ListItem<'i>);

impl<'i> From<ListItem<'i>> for Item<'i> {
    fn from(item: ListItem<'i>) -> Self {
        Item(item)
    }
}

impl<'i> From<Item<'i>> for ListItem<'i> {
    fn from(item: Item<'i>) -> Self {
        item.0
    }
}

#[derive(Clone, Debug)]
pub struct List<'t> {
    pub items: Vec<Item<'t>>,
    state: ListState,
}

impl<'t> List<'t> {
    pub fn new(items: Option<Vec<Item<'t>>>) -> List<'t> {
        if let Some(i) = items {
            List {
                items: i,
                state: ListState::default(),
            }
        } else {
            List {
                items: Vec::new(),
                state: ListState::default(),
            }
        }
    }

    pub fn list_items(&self) -> Vec<ListItem<'t>> {
        self.items
            .iter()
            .map(|item| item.clone().into())
            .collect::<Vec<ListItem<'_>>>()
    }

    pub fn set_items(&mut self, items: Vec<Item<'t>>) {
        if let Some(selected) = self.state.selected() {
            if selected > items.len() {
                self.state.select(Some(items.len()));
            } else {
                self.state.select(Some(selected))
            }
        } else {
            self.state.select(Some(0));
        }
        self.items = items;
    }

    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if self.items.is_empty() {
                    0
                } else if i >= self.items.len() - 1 {
                    self.items.len() - 1
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if self.items.is_empty() || i == 0 {
                    0
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn selected(&self) -> Option<usize> {
        self.state.selected()
    }

    #[allow(unused)]
    pub fn select(&mut self, num: usize) {
        self.state.select(Some(num));
    }
}

#[derive(Debug, Clone)]
pub struct Row {
    columns: Vec<String>,
    widths: Vec<ColumnWidth>,
}

impl Row {
    pub fn new(columns: Vec<String>, widths: Vec<ColumnWidth>) -> Row {
        Row { columns, widths }
    }

    pub fn term_row(&self, size: u16) -> TermRow<'_> {
        let column_widths = self
            .widths
            .iter()
            .map(|w| (size as f64 * (w.column_size as f64 * 0.01)).floor() as u16)
            .collect::<Vec<u16>>();

        let formatted = self
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let width = column_widths.get(i).unwrap();

                fill(c, *width as usize)
            })
            .collect::<Vec<String>>();

        let height = formatted
            .iter()
            .map(|f| {
                let count = f.matches('\n').count();

                if count == 0 {
                    1
                } else {
                    count + 1
                }
            })
            .max()
            .unwrap_or(1);

        TermRow::new(formatted)
            .style(Style::default())
            .height(height as u16)
    }
}

#[derive(Debug, Clone)]
pub struct ColumnWidth {
    /// Table column size in percent
    column_size: u16,
    constraint: Constraint,
}

impl ColumnWidth {
    /// Column sizes are in percent.
    pub fn new(column_size: u16) -> Self {
        ColumnWidth {
            column_size,
            constraint: Constraint::Percentage(column_size),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Table {
    rows: Vec<Row>,
    header: Vec<String>,
    state: TableState,
    widths: Vec<ColumnWidth>,
}

pub trait TableRows {
    fn rows(&self) -> Vec<Row>;
}

pub trait TableRow {
    fn row(&self) -> Row;
}

pub trait TableHeaders {
    fn headers() -> Vec<String>;
}

pub trait TableWidths {
    fn widths() -> Vec<ColumnWidth>;
}

impl Table {
    pub fn new(
        header: Option<Vec<String>>,
        items: Option<Vec<Row>>,
        widths: Option<Vec<ColumnWidth>>,
    ) -> Table {
        if let (Some(i), Some(header), Some(widths)) = (items, header, widths) {
            Table {
                rows: i,
                state: TableState::default(),
                header,
                widths,
            }
        } else {
            Table {
                rows: Vec::new(),
                state: TableState::default(),
                header: vec![],
                widths: vec![],
            }
        }
    }

    fn term_table(&self, size: u16) -> (Vec<TermRow>, Vec<Constraint>, Vec<String>) {
        let rows = self.term_rows(size);
        let widths = self
            .widths
            .iter()
            .map(|w| w.constraint)
            .collect::<Vec<Constraint>>();
        let header = self.header.clone();

        (rows, widths, header)
    }

    fn term_rows(&self, size: u16) -> Vec<TermRow> {
        self.rows
            .iter()
            .map(move |r| r.term_row(size))
            .collect::<Vec<TermRow>>()
    }

    pub fn set_header(&mut self, header: Vec<String>) {
        self.header = header;
    }

    pub fn set_rows(&mut self, rows: Vec<Row>) {
        self.rows = rows;
    }

    pub fn set_widths(&mut self, widths: Vec<ColumnWidth>) {
        self.widths = widths;
    }

    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if self.rows.is_empty() {
                    0
                } else if i >= self.rows.len() - 1 {
                    self.rows.len() - 1
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if self.rows.is_empty() || i == 0 {
                    0
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn selected(&self) -> Option<usize> {
        self.state.selected()
    }

    #[allow(unused)]
    pub fn select(&mut self, num: usize) {
        self.state.select(Some(num));
    }
}