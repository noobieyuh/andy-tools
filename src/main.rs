use std::{
    fs,
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};

use chrono::Local;
use color_eyre::Result;
use crossterm::event::{self, KeyCode};
use ratatui::{
    prelude::*,
    widgets::{Block, Cell, Paragraph, Row, Table, Tabs},
};

struct RunningProcess {
    name: String,
    pid: u32,
    child: Child,
    logs: Arc<Mutex<Vec<String>>>,
    exited: bool,
}

fn main() -> Result<()> {
    // fix to clear terminal
    print!("{}[2J", 27 as char);

    color_eyre::install()?;

    let mut selected_tab = 0;
    let mut selected_script = 0;
    let mut selected_process = 0;
    let mut log_scroll: i32 = 0;

    let mut scripts = load_scripts();
    let mut running: Vec<RunningProcess> = Vec::new();

    ratatui::run(|terminal| loop {
        update_process_statuses(&mut running);
        scripts = load_scripts();

        terminal.draw(|frame| {
            render(
                frame,
                selected_tab,
                selected_script,
                selected_process,
                log_scroll,
                &scripts,
                &running,
            )
        })?;

        if let Some(key) = event::read()?.as_key_press_event() {
            match key.code {
                // esc to close program
                KeyCode::Esc => break Ok(()),

                // switch to the tab on the right
                KeyCode::Right => selected_tab = (selected_tab + 1) % 3,

                // switch to the tab on the left
                KeyCode::Left => selected_tab = (selected_tab + 2) % 3,

                // up to select scripts
                KeyCode::Up => {
                    if selected_tab == 0 {
                        selected_script = selected_script.saturating_sub(1);
                    } else if selected_tab == 1 {
                        selected_process = selected_process.saturating_sub(1);
                    } else if selected_tab == 2 {
                        log_scroll += 1;
                    }
                }

                // down to select scripts
                KeyCode::Down => {
                    if selected_tab == 0 {
                        selected_script = (selected_script + 1).min(scripts.len().saturating_sub(1));
                    } else if selected_tab == 1 {
                        selected_process =
                            (selected_process + 1).min(running.len().saturating_sub(1));
                    } else if selected_tab == 2 {
                        log_scroll = (log_scroll - 1).max(0);
                    }
                }


                // logic for logs

                KeyCode::PageUp => {
                    if selected_tab == 2 {
                        log_scroll += 10;
                    }
                }
                KeyCode::PageDown => {
                    if selected_tab == 2 {
                        log_scroll = (log_scroll - 10).max(0);
                    }
                }


                // enter to activate script
                KeyCode::Enter => {
                    if selected_tab == 0 && !scripts.is_empty() {
                        run_python_script(&scripts[selected_script], &mut running)?;
                    }
                }

                // k to kill script
                KeyCode::Char('k') => {
                    if selected_tab == 1 && !running.is_empty() {
                        kill_process(&mut running, selected_process);
                    }
                }

                // c to clear logs
                KeyCode::Char('c') => {
                    if selected_tab == 2 {
                        for p in &running {
                            p.logs.lock().unwrap().clear();
                        }
                        log_scroll = 0;
                    }
                }

                _ => {}
            }
        }
    })
}

fn load_scripts() -> Vec<String> {
    let mut list = Vec::new();


    // auto find all python scripts in scripts folder and add them to tui
    if let Ok(entries) = fs::read_dir("scripts") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "py").unwrap_or(false) {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    list.push(name.to_string());
                }
            }
        }
    }

    list.sort();
    list
}

// update the process status - ex. complete -> exit

fn update_process_statuses(running: &mut Vec<RunningProcess>) {
    for p in running.iter_mut() {
        if !p.exited {
            if let Ok(Some(_)) = p.child.try_wait() {
                p.exited = true;
                let ts = Local::now().format("%Y-%m-%d %H:%M:%S");
                p.logs
                    .lock()
                    .unwrap()
                    .push(format!("[{}] Process exited", ts));
            }
        }
    }
}

// kill process logic

fn kill_process(running: &mut Vec<RunningProcess>, index: usize) {
    if let Some(p) = running.get_mut(index) {
        if !p.exited {
            let _ = p.child.kill();
            p.exited = true;
            let ts = Local::now().format("%Y-%m-%d %H:%M:%S");
            p.logs
                .lock()
                .unwrap()
                .push(format!("[{}] Process killed by user", ts));
        }
    }
}

// draw all elements onto the screen

fn render(
    frame: &mut Frame,
    selected_tab: usize,
    selected_script: usize,
    selected_process: usize,
    log_scroll: i32,
    scripts: &[String],
    running: &[RunningProcess],
) {
    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .spacing(1);

    let [title_area, tabs_area, content_area] = frame.area().layout(&layout);

    let title = Line::from_iter([Span::from(" Andy's Toolbox - Press 'esc' to quit").bold()]);
    frame.render_widget(title.centered(), title_area);

    render_tabs(frame, tabs_area, selected_tab);
    render_content(
        frame,
        content_area,
        selected_tab,
        selected_script,
        selected_process,
        log_scroll,
        scripts,
        running,
    );
}

fn render_tabs(frame: &mut Frame, area: Rect, selected_tab: usize) {
    let tabs = Tabs::new(vec!["Scripts", "Processes", "Logs"])
        .style(Color::White)
        .highlight_style(Style::default().magenta().on_black().bold())
        .select(selected_tab)
        .divider(symbols::DOT)
        .padding(" ", " ");

    frame.render_widget(tabs, area);
}

fn render_content(
    frame: &mut Frame,
    area: Rect,
    selected_tab: usize,
    selected_script: usize,
    selected_process: usize,
    log_scroll: i32,
    scripts: &[String],
    running: &[RunningProcess],
) {
    match selected_tab {
        0 => render_scripts(frame, area, selected_script, scripts, running),
        1 => render_processes(frame, area, running, selected_process),
        2 => render_logs(frame, area, running, log_scroll),
        _ => unreachable!(),
    }
}

// function to check if there are scripts running at all

fn is_script_running(name: &str, running: &[RunningProcess]) -> bool {
    running.iter().any(|p| p.name == name && !p.exited)
}

// logic for inactive and active scripts

fn render_scripts(
    frame: &mut Frame,
    area: Rect,
    selected_script: usize,
    scripts: &[String],
    running: &[RunningProcess],
) {
    let rows = scripts.iter().enumerate().map(|(i, name)| {
        let style = if i == selected_script {
            Style::default().fg(Color::Black).bg(Color::Yellow)
        } else {
            Style::default()
        };

        let status = if is_script_running(name, running) {
            "Active"
        } else {
            "Inactive"
        };

        Row::new(vec![Cell::from(name.clone()), Cell::from(status)]).style(style)
    });

    let table = Table::new(rows, [Constraint::Percentage(60), Constraint::Percentage(40)])
        .header(
            Row::new(vec!["Name", "Status"])
                .style(Style::default().fg(Color::Yellow).bold()),
        )
        .block(
            Block::bordered()
                .title(" Scripts - Press 'enter' to select ")
                .title_alignment(Alignment::Center),
        );

    frame.render_widget(table, area);
}

// logic for if script process is running or completed/exited

fn render_processes(
    frame: &mut Frame,
    area: Rect,
    running: &[RunningProcess],
    selected_process: usize,
) {
    let rows = running.iter().enumerate().map(|(i, p)| {
        let style = if i == selected_process {
            Style::default().fg(Color::Black).bg(Color::Red)
        } else {
            Style::default()
        };

        let status = if p.exited { "Exited" } else { "Running" };

        Row::new(vec![
            Cell::from(p.name.clone()),
            Cell::from(p.pid.to_string()),
            Cell::from(status),
        ])
        .style(style)
    });

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(40),
            Constraint::Percentage(20),
            Constraint::Percentage(40),
        ],
    )
    .header(
        Row::new(vec!["Name", "PID", "Status"])
            .style(Style::default().fg(Color::Yellow).bold()),
    )
    .block(
        Block::bordered()
            .title(" Processes - Press 'k' to kill ")
            .title_alignment(Alignment::Center),
    );

    frame.render_widget(table, area);
}

// rendering logs onto tui

fn render_logs(frame: &mut Frame, area: Rect, running: &[RunningProcess], scroll: i32) {
    let mut all_logs = Vec::new();

    for p in running {
        let logs = p.logs.lock().unwrap();
        for line in logs.iter() {
            all_logs.push(format!("[PID {}] {} {}", p.pid, p.name, line));
        }
    }

    if all_logs.is_empty() {
        all_logs.push("No logs yet...".to_string());
    }

    let total = all_logs.len() as i32;
    let height = area.height as i32 - 2;

    let max_scroll = (total - height).max(0);

    let scroll_pos = if scroll < 0 {
        0
    } else if scroll > max_scroll {
        max_scroll
    } else {
        scroll
    };

    let start_i32 = (total - height - scroll_pos).max(0);
    let end_i32 = (start_i32 + height).min(total);

    let start = start_i32 as usize;
    let end = end_i32 as usize;

    let visible = &all_logs[start..end];
    let text = visible.join("\n");

    let block = Paragraph::new(text)
        .block(
            Block::bordered()
                .title(" Logs - Press 'c' to clear ")
                .title_alignment(Alignment::Center),
        )
        .alignment(Alignment::Left);

    frame.render_widget(block, area);
}

// run python script once selected with enter

fn run_python_script(script: &str, running: &mut Vec<RunningProcess>) -> Result<()> {
    if running.iter().any(|p| p.name == script && !p.exited) {
        return Ok(());
    }

    let mut child = Command::new("python3")
        .arg(format!("scripts/{script}"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let pid = child.id();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let logs = Arc::new(Mutex::new(Vec::new()));
    let logs_out = logs.clone();
    let logs_err = logs.clone();

    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(line) = line {
                let ts = Local::now().format("%Y-%m-%d %H:%M:%S");
                logs_out
                    .lock()
                    .unwrap()
                    .push(format!("[{}] [OUT] {}", ts, line));
            }
        }
    });

    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(line) = line {
                let ts = Local::now().format("%Y-%m-%d %H:%M:%S");
                logs_err
                    .lock()
                    .unwrap()
                    .push(format!("[{}] [ERR] {}", ts, line));
            }
        }
    });

    running.push(RunningProcess {
        name: script.to_string(),
        pid,
        child,
        logs,
        exited: false,
    });

    Ok(())
}
