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
        }
    }

    /// Single-glyph brand mark for chat lines.
    pub fn mark(a: Agent) -> &'static str {
        match a {
            Agent::Claude => "\u{2733}",
            Agent::Codex => "\u{2B21}",
            Agent::Mistral => "\u{004D}\u{030A}",
        }
    }

    pub fn color(a: Agent) -> Color {
        match a {
            Agent::Claude => Color::Rgb(217, 119, 87),
            Agent::Codex => Color::Rgb(16, 163, 127),
            Agent::Mistral => Color::Rgb(250, 200, 60),
        }
    }

    /// Animated multi-line brand sprite, one frame per call.
    fn sprite(a: Agent, frame: usize, act: Act) -> Vec<Line<'static>> {
        let style = Style::default().fg(Self::color(a));
        match a {
            Agent::Claude => {
                let f = match act {
                    Act::Idle => (frame / 4) % 2,
                    _ => frame % 2,
                };
                let rows = if f == 0 {
                    vec![" \\ | / ", "  \u{2733}  ", " / | \\"]
                } else {
                    vec![" / \u{2014} \\ ", "\u{2014} \u{2733} \u{2014}", " \\ \u{2014} / "]
                };
                rows.into_iter()
                    .map(|r| Line::from(Span::styled(r.to_string(), style)))
                    .collect()
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
                vec![
                    Line::from(Span::styled(
                        format!("  {}   {}  ", node(0), node(1)),
                        style,
                    )),
                    Line::from(Span::styled(
                        format!("{}   \u{2726}   {}", node(2), node(3)),
                        style,
                    )),
                    Line::from(Span::styled(
                        format!("  {}   {}  ", node(4), node(5)),
                        style,
                    )),
                ]
            }
            Agent::Mistral => {
                const PATTERNS: [[u8; 4]; 4] =
                    [[3, 1, 2, 3], [2, 3, 1, 2], [1, 2, 3, 2], [2, 1, 3, 1]];
                let p = match act {
                    Act::Idle => PATTERNS[0],
                    _ => PATTERNS[frame % PATTERNS.len()],
                };
                (0..3)
                    .rev()
                    .map(|row| {
                        let mut s = String::new();
                        for h in p {
                            s.push_str(if (h as i32) > row {
                                "\u{2588}\u{2588}"
                            } else {
                                "  "
                            });
                        }
                        Line::from(Span::styled(s, style))
                    })
                    .collect()
            }
        }
    }

    pub fn push(&mut self, ev: Event) {
        let mark_act = |me: &mut Self, agent: Agent, a: Act| {
            if let Some(act) = me.activity.get_mut(&agent) {
                act.act = a;
                act.since = Instant::now();
            }
        };
        match ev {
            Event::Said(agent, text) => {
                mark_act(self, agent, Act::Talking);
                self.lines.push(Line::from(Span::styled(
                    format!(" {} {} ", Self::mark(agent), agent.name()),
                    Style::default()
                        .fg(Self::color(agent))
                        .add_modifier(Modifier::BOLD),
                )));
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
                    format!(" {} {} is thinking", Self::mark(agent), agent.name()),
                    Style::default().fg(Self::color(agent)),
                )));
            }
            Event::Working(agent) => {
                mark_act(self, agent, Act::Working);
                self.lines.push(Line::from(Span::styled(
                    format!(" {} {} gets to work", Self::mark(agent), agent.name()),
                    Style::default().fg(Self::color(agent)),
                )));
            }
            Event::Director(agent) => {
                self.lines.push(Line::from(Span::styled(
                    format!(
                        "\u{1F3D4} {} {} takes the director's seat",
                        Self::mark(agent),
                        agent.name()
                    ),
                    Style::default()
                        .fg(Self::color(agent))
                        .add_modifier(Modifier::BOLD),
                )));
            }
            Event::Contribution(agent, bytes) => {
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
                    ("KEEP", Color::LightGreen)
                } else {
                    ("REJECT", Color::LightRed)
                };
                self.lines.push(Line::from(Span::styled(
                    format!(
                        " {} {} reviews {}'s diff: {verdict} \u{2014} {reason}",
                        Self::mark(director),
                        director.name(),
                        helper.name()
                    ),
                    Style::default().fg(col),
                )));
            }
            Event::Applied(helper) => {
                self.lines.push(Line::from(Span::styled(
                    format!("\u{1F33E} {}'s harvest applied", helper.name()),
                    Style::default().fg(Color::LightYellow),
                )));
            }
            Event::NoKeep => {
                self.lines.push(Line::from(Span::styled(
                    "\u{1F937} no diff kept \u{2014} this field needs human eyes",
                    Style::default().fg(Color::LightRed),
                )));
            }
            Event::System(text) => {
                self.lines.push(Line::from(Span::styled(
                    format!("\u{2500}\u{2500} {text} \u{2500}\u{2500}"),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }
    }

    pub fn tick(&mut self) -> bool {
        if self.last_tick.elapsed() >= Duration::from_millis(120) {
            self.frame += 1;
            self.last_tick = Instant::now();
            for act in self.activity.values_mut() {
                if act.since.elapsed() > Duration::from_millis(1500) && act.act != Act::Idle {
                    act.act = Act::Idle;
                }
            }
            true
        } else {
            false
        }
    }

    pub fn render(&self, area: Rect, f: &mut ratatui::Frame) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(40), Constraint::Length(24)])
            .split(area);

        self.render_chat(chunks[0], f);
        self.render_sidebar(chunks[1], f);
    }

    fn render_chat(&self, area: Rect, f: &mut ratatui::Frame) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" \u{1F3D4} imece village ");
        let visible_h = area.height.saturating_sub(2) as usize;
        let skip = self.lines.len().saturating_sub(visible_h);
        let para = Paragraph::new(self.lines[skip..].to_vec())
            .wrap(Wrap { trim: false })
            .block(block);
        f.render_widget(para, area);
    }

    fn render_sidebar(&self, area: Rect, f: &mut ratatui::Frame) {
        let block = Block::default().borders(Borders::ALL).title(" villagers ");
        let mut lines: Vec<Line> = vec![];
        for agent in crate::imece::VILLAGE {
            let idle = Activity {
                act: Act::Idle,
                since: Instant::now(),
            };
            let act = self.activity.get(&agent).unwrap_or(&idle);
            let status = match act.act {
                Act::Idle => "idle",
                Act::Thinking => "thinking",
                Act::Working => "working",
                Act::Talking => "talking",
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {} {} ", Self::mark(agent), agent.name()),
                    Style::default()
                        .fg(Self::color(agent))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(status.to_string(), Style::default().fg(Color::DarkGray)),
            ]));
            lines.extend(Self::sprite(agent, self.frame, act.act));
            lines.push(Line::from(""));
        }
        let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
        f.render_widget(para, area);
    }
}
