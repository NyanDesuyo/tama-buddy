use crate::App;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Wrap},
};

const MINT: Color = Color::Rgb(130, 220, 185);
const MUTED: Color = Color::Rgb(139, 153, 172);
const BG: Color = Color::Rgb(20, 26, 38);
const GOLD: Color = Color::Rgb(246, 204, 122);
fn panel(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .title(format!(" {title} "))
        .border_style(Style::default().fg(Color::Rgb(61, 78, 96)))
}
pub fn draw(f: &mut Frame, app: &App, now: i64) {
    let area = f.area();
    f.render_widget(
        Block::default().style(Style::default().bg(BG).fg(Color::White)),
        area,
    );
    if area.width < 60 || area.height < 30 {
        f.render_widget(Paragraph::new("TAMA BUDDY\n\nPlease resize to at least 60 × 30.\nYour buddy is still being cared for.\nEsc / Ctrl+C to quit.").alignment(Alignment::Center).wrap(Wrap { trim: true }), area);
        return;
    }
    let [header, body, actions, message, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(16),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .margin(1)
    .areas(area);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" TAMA BUDDY ", Style::default().fg(MINT).bold()),
            Span::styled(
                " / a little life, a little love",
                Style::default().fg(MUTED),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(MUTED)),
        ),
        header,
    );
    if app.naming {
        f.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from("A new friendship is waiting.").fg(MINT).bold(),
                Line::from(""),
                Line::from("       .-''''-.       "),
                Line::from("      /  .  .  \\      "),
                Line::from("     |    __    |     "),
                Line::from("      \\        /      "),
                Line::from("       '------'       "),
                Line::from(""),
                Line::from("What should we call your buddy?"),
                Line::from(""),
                Line::from(format!("> {}_", app.name)).fg(GOLD),
                Line::from("Enter to hatch · blank name = Mochi").fg(MUTED),
            ])
            .alignment(Alignment::Center)
            .block(panel("THE NURSERY")),
            body,
        );
    } else if let Some(p) = &app.pet {
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
                .areas(body);
        let [habitat, journal] =
            Layout::vertical([Constraint::Min(10), Constraint::Length(6)]).areas(left);
        let eyes = if !p.alive() {
            "x   x"
        } else if p.sleeping || app.frame.is_multiple_of(24) {
            "-   -"
        } else {
            "o   o"
        };
        let bounce = if (app.frame / 4).is_multiple_of(2) && !p.sleeping && p.alive() {
            " "
        } else {
            ""
        };
        let age = p.age(now) / 60;
        let art = vec![
            Line::from(""),
            Line::from(format!("{}  /\\___/\\", bounce)).fg(MINT),
            Line::from(format!("{} ( {eyes} )", bounce)).fg(MINT),
            Line::from(format!(
                "{} (   {}   )",
                bounce,
                if p.sleeping { "z" } else { "w" }
            ))
            .fg(MINT),
            Line::from(format!("{}  /|     |\\", bounce)).fg(MINT),
            Line::from(format!("{} (_|_____|_)", bounce)).fg(MINT),
            Line::from(""),
            Line::from(p.name.as_str()).fg(GOLD).bold(),
            Line::from(format!(
                "{} · {}h {}m · Bond {}",
                p.stage(now),
                age / 60,
                age % 60,
                p.bond_level()
            ))
            .fg(MUTED),
            Line::from(p.mood()).fg(if p.health < 40 { Color::LightRed } else { MINT }),
        ];
        f.render_widget(
            Paragraph::new(art)
                .alignment(Alignment::Center)
                .block(panel("HOME SWEET HOME")),
            habitat,
        );
        let history: Vec<Line> = app
            .events
            .iter()
            .take(4)
            .map(|e| Line::from(format!(" · {e}")).fg(MUTED))
            .collect();
        f.render_widget(
            Paragraph::new(history).block(panel("LITTLE MEMORIES")),
            journal,
        );
        let inner = panel("WELLBEING").inner(right);
        f.render_widget(panel("WELLBEING"), right);
        let rows = Layout::vertical([Constraint::Length(3); 5])
            .margin(1)
            .split(inner);
        for (i, (label, value)) in [
            ("Fullness", p.fullness),
            ("Happiness", p.happiness),
            ("Energy", p.energy),
            ("Hygiene", p.hygiene),
            ("Health", p.health),
        ]
        .iter()
        .enumerate()
        {
            let color = if *value < 25 {
                Color::LightRed
            } else if *value < 50 {
                GOLD
            } else {
                MINT
            };
            f.render_widget(
                Gauge::default()
                    .block(Block::default().title(*label))
                    .gauge_style(Style::default().fg(color).bg(Color::Rgb(40, 48, 62)))
                    .percent((*value).clamp(0, 100) as u16)
                    .label(format!("{value}/100")),
                rows[i],
            );
        }
    }
    let labels = ["[F] Feed", "[P] Play", "[C] Clean", "[S] Sleep", "[M] Meds"];
    let spans: Vec<Span> = labels
        .iter()
        .enumerate()
        .map(|(i, label)| {
            Span::styled(
                format!(" {label} "),
                if app.selected == i {
                    Style::default().fg(BG).bg(MINT).bold()
                } else {
                    Style::default().fg(MUTED)
                },
            )
        })
        .collect();
    f.render_widget(
        Paragraph::new(Line::from(spans))
            .alignment(Alignment::Center)
            .block(panel("CARE")),
        actions,
    );
    let text = app.message.as_str();
    f.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(GOLD))
            .block(panel("A MOMENT TOGETHER")),
        message,
    );
    f.render_widget(
        Paragraph::new("← → select · Enter act · Q/Esc quit · Backend autosave")
            .alignment(Alignment::Center)
            .fg(MUTED),
        footer,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    #[test]
    fn draws_nursery_dashboard_and_small_terminal() {
        let mut app = App {
            pet: None,
            name: String::new(),
            naming: true,
            selected: 0,
            message: "Hello".into(),
            events: vec![],
            frame: 0,
        };
        for (w, h) in [(80, 32), (60, 30), (30, 10)] {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|f| draw(f, &app, 1000)).unwrap();
            app.naming = false;
            app.pet = Some(crate::pet::Pet::new("Mochi".into(), 1000));
            terminal.draw(|f| draw(f, &app, 1000)).unwrap();
            let content: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect();
            assert!(content.contains(if w < 60 { "resize" } else { "Mochi" }));
            app.naming = true;
        }
    }
}
