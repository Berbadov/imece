use crate::imece::{Agent, Event};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Chat-style face of the village, with each model's real brand mark.
pub struct Chat {
    pub lines: Vec<Line<'static>>,
    activity: HashMap<Agent, Activity>,
    frame: usize,
    last_tick: Instant,
    // scroll state: None = stick to bottom; Some(offset) = scrolled up
    scroll: Option<u16>,
    paused: bool,
    // task progress
    task_idx: usize,
    task_total: usize,
    task_name: String,
    director: Option<Agent>,
    director_of: HashMap<Agent, u32>,
    diffs_kept: u32,
    diffs_total: u32,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Act {
    Idle,
    Thinking,
    Working,
    Talking,
}

struct Activity {
    act: Act,
    since: Instant,
}

impl Act {
    fn label(&self) -> &'static str {
        match self {
            Act::Idle => "idle",
            Act::Thinking => "thinking",
            Act::Working => "working",
            Act::Talking => "talking",
        }
    }
}

impl Chat {
    pub fn new() -> Self {
        let mut activity = HashMap::new();
        for a in crate::imece::VILLAGE {
            activity.insert(
                a,
                Activity {
                    act: Act::Idle,
                    since: Instant::now(),
                },
            );
        }
        Self {
            lines: vec![],
            activity,
            frame: 0,
            last_tick: Instant::now(),
            scroll: None,
            paused: false,
            task_idx: 0,
            task_total: 0,
            task_name: String::new(),
            director: None,
            director_of: HashMap::new(),
            diffs_kept: 0,
            diffs_total: 0,
        }
    }

    pub fn mark(a: Agent) -> &'static str {
        match a {
            Agent::Claude => "\u{2733}",
            Agent::Codex => "\u{2B21}",
            Agent::Mistral => "\u{004D}",
        }
    }

    pub fn color(a: Agent) -> Color {
        match a {
            Agent::Claude => Color::Rgb(217, 119, 87),
            Agent::Codex => Color::Rgb(16, 163, 127),
            Agent::Mistral => Color::Rgb(250, 200, 60),
        }
    }

    pub fn key_hints() -> Line<'static> {
        Line::from(vec![
            Span::styled(" \u{2191}\u{2193} scroll ", Style::default().fg(Color::DarkGray)),
            Span::styled("g/G ", Style::default().fg(Color::Gray)),
            Span::styled("top/bottom ", Style::default().fg(Color::DarkGray)),
            Span::styled("space ", Style::default().fg(Color::Gray)),
            Span::styled("pause ", Style::default().fg(Color::DarkGray)),
            Span::styled("r ", Style::default().fg(Color::Gray)),
            Span::styled("resume ", Style::default().fg(Color::DarkGray)),
            Span::styled("q ", Style::default().fg(Color::Gray)),
            Span::styled("quit ", Style::default().fg(Color::DarkGray)),
        ])
    }

    fn brand_name(a: Agent) -> String {
        let avatar = Self::mark(a);
        format!("{avatar} {}", a.name())
    }

    pub fn push(&mut self, ev: Event) {
        if self.paused {
            return;
        }
        let mark_act = |me: &mut Self, agent: Agent, a: Act| {
            if let Some(act) = me.activity.get_mut(&agent) {
                act.act = a;
                act.since = Instant::now();
            }
        };
        match ev {
            Event::Said(agent, text) => {
                mark_act(self, agent, Act::Talking);
                self.bubble_header(agent);
                for line in text.lines() {
                    self.lines.push(Line::from(Span::styled(
                        format!("  {line}"),
                        Style::default().fg(Self::color(agent)),
                    )));
                }
                self.lines.push(Line::from(""));
            }
            Event::Thinking(agent) => {
                mark_act(self, agent, Act::Thinking);
                self.lines.push(Line::from(Span::styled(
                    format!(" {} {} is thinking\u{2026}", Self::mark(agent), agent.name()),
                    Style::default().fg(Self::color(agent)),
                )));
            }
            Event::Working(agent) => {
                mark_act(self, agent, Act::Working);
                self.lines.push(Line::from(Span::styled(
                    format!(" {} {} rolls up sleeves\u{2026}", Self::mark(agent), agent.name()),
                    Style::default().fg(Self::color(agent)),
                )));
            }
            Event::Director(agent) => {
                self.director = Some(agent);
                *self.director_of.entry(agent).or_insert(0) += 1;
                self.lines.push(Line::from(vec![
                    Span::styled("\u{1F3D4} ", Style::default()),
                    Span::styled(
                        format!("{} takes the director's seat", Self::brand_name(agent)),
                        Style::default()
                            .fg(Self::color(agent))
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(
                            "  \u{2014} task {}/{}",
                            self.task_idx, self.task_total
                        ),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }
            Event::Contribution(agent, bytes) => {
                self.diffs_total += 1;
                self.lines.push(Line::from(Span::styled(
                    format!(
                        " {} {} contributes a diff ({bytes} bytes)",
                        Self::mark(agent),
                        agent.name()
                    ),
                    Style::default().fg(Self::color(agent)),
                )));
            }
            Event::Review(director, helper, keep, reason) => {
                let (verdict, col) = if keep {
                    ("\u{2705} KEEP", Color::LightGreen)
                } else {
                    ("\u{274C} REJECT", Color::LightRed)
                };
                self.lines.push(Line::from(vec![
                    Span::styled(
                        format!(
                            " {} {} reviews {}'s: ",
                            Self::mark(director),
                            director.name(),
                            helper.name()
                        ),
                        Style::default().fg(Self::color(director)),
                    ),
                    Span::styled(verdict, Style::default().fg(col).add_modifier(Modifier::BOLD)),
                    Span::styled(format!(" \u{2014} {reason}"), Style::default().fg(Color::Gray)),
                ]));
            }
            Event::Applied(helper) => {
                self.diffs_kept += 1;
                self.lines.push(Line::from(vec![
                    Span::styled(
                        "\u{1F33E} ",
                        Style::default(),
                    ),
                    Span::styled(
                        format!("{}'s harvest applied", helper.name()),
                        Style::default().fg(Color::LightYellow),
                    ),
                    Span::styled(
                        format!("  ({}/{} kept)", self.diffs_kept, self.diffs_total),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }
            Event::NoKeep => {
                self.lines.push(Line::from(Span::styled(
                    "\u{1F937} no diff kept \u{2014} this field needs human eyes",
                    Style::default().fg(Color::LightRed),
                )));
            }
            Event::System(text) => match text.split_once(' ').map(|(k, rest)| (k, rest)) {
                Some(("queue", rest)) => {
                    if let Ok(n) = rest.trim().split(' ').next().unwrap_or("0").parse::<usize>() {
                        self.task_total = n;
                    }
                }
                Some(("task", rest)) => {
                    self.task_idx += 1;
                    self.task_name = rest.to_string();
                    self.director = None;
                    self.lines.push(Line::from(vec![
                        Span::styled(
                            "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::styled(format!(" {rest} "), Style::default().fg(Color::White)),
                        Span::styled(
                            "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]));
                }
                _ => {
                    self.lines.push(Line::from(Span::styled(
                        format!("  {text}"),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            },
        }
    }

    fn bubble_header(&mut self, agent: Agent) {
        let star = if self.director == Some(agent) {
            "\u{1F451}"
        } else {
            ""
        };
        let header = format!(" {} {} {star}", Self::mark(agent), agent.name());
        self.lines.push(Line::from(Span::styled(
            header,
            Style::default()
                .fg(Self::color(agent))
                .add_modifier(Modifier::BOLD),
        )));
    }

    pub fn tick(&mut self) -> bool {
        if self.last_tick.elapsed() >= Duration::from_millis(120) {
            self.frame += 1;
            self.last_tick = Instant::now();
            for act in self.activity.values_mut() {
                if act.since.elapsed() > Duration::from_millis(1800) && act.act != Act::Idle {
                    act.act = Act::Idle;
                }
            }
            true
        } else {
            false
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll = Some(self.scroll.unwrap_or(0).saturating_add(3));
    }

    pub fn scroll_down(&mut self) {
        if let Some(s) = self.scroll {
            self.scroll = if s <= 3 { None } else { Some(s - 3) };
        }
    }

    pub fn scroll_top(&mut self) {
        self.scroll = Some(u16::MAX);
    }

    pub fn scroll_bottom(&mut self) {
        self.scroll = None;
    }

    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    pub fn render(&self, area: Rect, f: &mut ratatui::Frame) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
            .split(area);

        self.render_header(rows[0], f);
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(50), Constraint::Percentage(32)])
            .split(rows[1]);
        self.render_chat(cols[0], f);
        self.render_sidebar(cols[1], f);
        Self::render_footer(rows[2], f);
    }

    fn render_header(&self, area: Rect, f: &mut ratatui::Frame) {
        if area.width < 20 {
            return;
        }
        let mut spans = vec![
            Span::styled(" \u{1F3D4} imece ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ];
        if !self.task_name.is_empty() {
            spans.push(Span::styled(
                format!("\u{2502} {} ", self.task_name),
                Style::default().fg(Color::Gray),
            ));
            if self.task_total > 0 {
                spans.push(Span::styled(
                    format!("({}/{}) ", self.task_idx, self.task_total),
                    Style::default().fg(Color::DarkGray),
                ));
            }
        }
        if let Some(d) = self.director {
            spans.push(Span::styled(
                format!("\u{2502} director: {} {}", Self::mark(d), d.name()),
                Style::default().fg(Self::color(d)),
            ));
        }
        if self.paused {
            spans.push(Span::styled(
                "  \u{23F8} PAUSED",
                Style::default().fg(Color::LightRed).add_modifier(Modifier::BOLD),
            ));
        }
        let para = Paragraph::new(Line::from(spans));
        f.render_widget(para, area);
    }

    fn render_chat(&self, area: Rect, f: &mut ratatui::Frame) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(60, 60, 60)))
            .title(" \u{1F4AC} village chat ");
        let visible_h = area.height.saturating_sub(2) as usize;
        let total = self.lines.len();
        let skip = match self.scroll {
            None => total.saturating_sub(visible_h),
            Some(off) => {
                let from_top = total.saturating_sub(visible_h);
                from_top.saturating_sub((off as usize).min(from_top))
            }
        };
        let para = Paragraph::new(self.lines[skip..].to_vec())
            .wrap(Wrap { trim: false })
            .block(block);
        f.render_widget(para, area);
    }

    fn render_sidebar(&self, area: Rect, f: &mut ratatui::Frame) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(60, 60, 60)))
            .title(" villagers ");
        let mut lines: Vec<Line> = vec![];
        for agent in crate::imece::VILLAGE {
            let idle = Activity {
                act: Act::Idle,
                since: Instant::now(),
            };
            let act = self.activity.get(&agent).unwrap_or(&idle);
            let is_director = self.director == Some(agent);
            let crown = if is_director {
                Span::styled(
                    " \u{1F451} director",
                    Style::default().fg(Color::LightYellow).add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(
                    format!(" {}", act.act.label()),
                    Style::default().fg(Color::DarkGray),
                )
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {} {} ", Self::mark(agent), agent.name()),
                    Style::default()
                        .fg(Self::color(agent))
                        .add_modifier(Modifier::BOLD),
                ),
                crown,
            ]));
            lines.extend(Self::sprite(agent, self.frame, act.act));
            let directed = self.director_of.get(&agent).copied().unwrap_or(0);
            lines.push(Line::from(Span::styled(
                format!("  directed {directed}x"),
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::from(""));
        }
        let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
        f.render_widget(para, area);
    }

    fn render_footer(area: Rect, f: &mut ratatui::Frame) {
        if area.width < 40 {
            return;
        }
        let hints = Self::key_hints();
        f.render_widget(
            Paragraph::new(hints).style(Style::default().fg(Color::DarkGray)),
            area,
        );
    }

    fn sprite(a: Agent, frame: usize, act: Act) -> Vec<Line<'static>> {
        let style = Style::default().fg(Self::color(a));
        let dim = Style::default().fg(Color::Rgb(90, 90, 90));
        match a {
            Agent::Claude => {
                let f = match act {
                    Act::Idle => (frame / 5) % 2,
                    _ => frame % 2,
                };
                let (rays, mark) = if f == 0 {
                    (" \\ | / ", "\u{2014} \u{2733} \u{2014}")
                } else {
                    (" / \u{2014} \\ ", "  \u{2733}  ")
                };
                let st = match act {
                    Act::Idle => dim,
                    _ => style,
                };
                vec![
                    Line::from(Span::styled(rays.to_string(), st)),
                    Line::from(Span::styled(mark.to_string(), st)),
                    Line::from(Span::styled(" / | \\ ".to_string(), st)),
                ]
            }
            Agent::Codex => {
                let lit = match act {
                    Act::Idle => 0,
                    _ => frame % 6,
                };
                let node = |i: usize| -> &'static str {
                    if i == lit % 6 {
                        "\u{25CF}"
                    } else {
                        "\u{25CB}"
                    }
                };
                let st = |i: usize| -> Style {
                    if act == Act::Idle {
                        dim
                    } else if i == lit % 6 {
                        style.add_modifier(Modifier::BOLD)
                    } else {
                        style
                    }
                };
                vec![
                    Line::from(vec![
                        Span::styled("  ", Style::default()),
                        Span::styled(node(0).to_string(), st(0)),
                        Span::styled("\u{2014}\u{2014}", st(0)),
                        Span::styled(node(1).to_string(), st(1)),
                    ]),
                    Line::from(vec![
                        Span::styled(node(2).to_string(), st(2)),
                        Span::styled("\u{2014}\u{2014}\u{25C6}\u{2014}\u{2014}", st(2)),
                        Span::styled(node(3).to_string(), st(3)),
                    ]),
                    Line::from(vec![
                        Span::styled("  ", Style::default()),
                        Span::styled(node(4).to_string(), st(4)),
                        Span::styled("\u{2014}\u{2014}", st(4)),
                        Span::styled(node(5).to_string(), st(5)),
                    ]),
                ]
            }
            Agent::Mistral => {
                const PATTERNS: [[u8; 4]; 4] =
                    [[3, 1, 2, 3], [2, 3, 1, 2], [1, 2, 3, 2], [2, 1, 3, 1]];
                let p = match act {
                    Act::Idle => PATTERNS[0],
                    _ => PATTERNS[frame % PATTERNS.len()],
                };
                let st = match act {
                    Act::Idle => dim,
                    _ => style,
                };
                (0..3)
                    .rev()
                    .map(|row| {
                        let mut spans = vec![];
                        for h in p {
                            if (h as i32) > row {
                                spans.push(Span::styled("\u{2588}\u{2588}".to_string(), st));
                            } else {
                                spans.push(Span::styled("  ".to_string(), st));
                            }
                        }
                        Line::from(spans)
                    })
                    .collect()
            }
        }
    }
}
