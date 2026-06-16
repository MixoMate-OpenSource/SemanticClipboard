use iced::widget::{button, column, container, row, scrollable, text, text_input, toggler, Space, rule, mouse_area, svg};
use iced::{Color, Element, Length, Task, Theme, Subscription, Alignment};
use iced::window;
use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::{AtomicU8, AtomicBool, Ordering};
use arboard::Clipboard;

use crate::db::{ClipboardEntry, Database};
use crate::ml::MLEngine;



pub struct SemanticClipboardApp {
    db: Arc<Mutex<Database>>,
    ml: Arc<Mutex<MLEngine>>,
    search_query: String,
    results: Vec<ClipboardEntry>,
    is_visible: Arc<Mutex<bool>>,
    needs_refresh: Arc<AtomicBool>,
    command_flag: Arc<AtomicU8>,
    obscuring_id: Option<i64>,
    obscuring_text: String,
    show_settings: bool,
    history_limit_str: String,
    cache_only_pinned: bool,
    show_in_tray: bool,
    content_to_paste: Arc<Mutex<String>>,
    window_id: Option<iced::window::Id>,
    always_on_top: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    SearchChanged(String),
    ToggleSettings,
    ClearHistory,
    DragWindow,
    ToggleCachePinned(bool),
    LimitChanged(String),
    WindowId(iced::window::Id),
    Delete(i64),
    Obscure(i64),
    ObscuringTextChanged(String),
    SaveObscure(i64),
    CancelObscure,
    RemoveObscure(i64),
    Copy(String),
    TogglePin(i64),
    Tick,
    RunOnStartup,
    Unfocused,
    ToggleShowInTray(bool),
    ToggleAlwaysOnTop,
    MinimizeWindow,
}

impl SemanticClipboardApp {
    pub fn new(
        db: Arc<Mutex<Database>>,
        ml: Arc<Mutex<MLEngine>>,
        is_visible: Arc<Mutex<bool>>,
        needs_refresh: Arc<AtomicBool>,
        command_flag: Arc<AtomicU8>,
    ) -> Self {
        let results = db.lock().unwrap().get_all_entries().unwrap_or_default();
        let history_limit_str = db.lock().unwrap().get_setting("history_limit").unwrap_or_default().unwrap_or_else(|| "10000".to_string());
        let cache_only_pinned = db.lock().unwrap().get_setting("cache_only_pinned").unwrap_or_default() == Some("true".to_string());
        let show_in_tray = db.lock().unwrap().get_setting("show_in_tray").unwrap_or_default() == Some("true".to_string());

        Self {
            db,
            ml,
            search_query: String::new(),
            results,
            is_visible,
            needs_refresh,
            command_flag,
            obscuring_id: None,
            obscuring_text: String::new(),
            show_settings: false,
            history_limit_str,
            cache_only_pinned,
            show_in_tray,
            content_to_paste: Arc::new(Mutex::new(String::new())),
            window_id: None,
            always_on_top: true,
        }
    }

    pub fn update_search(&mut self) {
        let db = self.db.lock().unwrap();
        let all_entries = db.get_all_entries().unwrap_or_default();
        if self.search_query.is_empty() {
            self.results = all_entries;
        } else {
            let mut ml = self.ml.lock().unwrap();
            if let Ok(query_embed) = ml.embed(&self.search_query) {
                let mut scored: Vec<(f32, ClipboardEntry)> = all_entries.into_iter().filter_map(|e| {
                    if e.embedding.len() == query_embed.len() {
                        let score = crate::ml::MLEngine::cosine_similarity(&query_embed, &e.embedding);
                        Some((score, e))
                    } else {
                        None
                    }
                }).collect();
                scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
                self.results = scored.into_iter().map(|(_, e)| e).collect();
            } else {
                self.results = vec![];
            }
        }
    }
}

pub fn update(app: &mut SemanticClipboardApp, message: Message) -> Task<Message> {
    match message {
        Message::SearchChanged(query) => {
            app.search_query = query;
            app.update_search();
            Task::none()
        }
        Message::WindowId(id) => {
            app.window_id = Some(id);
            Task::none()
        }
        Message::Tick => {
            let cmd = app.command_flag.swap(0, Ordering::SeqCst);
            if cmd == 1 {
                let mut vis = app.is_visible.lock().unwrap();
                *vis = !*vis;
                if let Some(id) = app.window_id {
                    if *vis {
                        return Task::batch(vec![
                            iced::window::minimize(id, false),
                            iced::window::set_mode(id, iced::window::Mode::Windowed),
                            iced::window::gain_focus(id),
                        ]);
                    } else {
                        return iced::window::minimize(id, true);
                    }
                }
            } else if cmd == 4 {
                let mut vis = app.is_visible.lock().unwrap();
                *vis = true;
                if let Some(id) = app.window_id {
                    return Task::batch(vec![
                        iced::window::minimize(id, false),
                        iced::window::set_mode(id, iced::window::Mode::Windowed),
                        iced::window::gain_focus(id),
                    ]);
                }
            } else if cmd == 2 {
                let entries = app.db.lock().unwrap().get_all_entries().unwrap_or_default();
                if entries.len() > 1 {
                    if let Ok(mut cb) = Clipboard::new() {
                        let _ = cb.set_text(entries[1].content.clone());
                    }
                }
                *app.is_visible.lock().unwrap() = false;

                if let Some(id) = app.window_id {
                    return iced::window::minimize(id, true);
                }
            } else if app.needs_refresh.swap(false, Ordering::SeqCst) {
                app.update_search();
            }
            Task::none()
        }
        Message::ToggleSettings => {
            app.show_settings = !app.show_settings;
            Task::none()
        }
        Message::ClearHistory => {
            let _ = app.db.lock().unwrap().clear_unpinned_history();
            app.update_search();
            app.needs_refresh.store(true, Ordering::SeqCst);
            Task::none()
        }
        Message::DragWindow => {
            if let Some(id) = app.window_id {
                iced::window::drag(id)
            } else {
                Task::none()
            }
        }
        Message::ToggleCachePinned(b) => {
            app.cache_only_pinned = b;
            let val = if b { "true" } else { "false" };
            let _ = app.db.lock().unwrap().set_setting("cache_only_pinned", val);
            Task::none()
        }
        Message::ToggleShowInTray(b) => {
            app.show_in_tray = b;
            let val = if b { "true" } else { "false" };
            let _ = app.db.lock().unwrap().set_setting("show_in_tray", val);
            Task::none()
        }
        Message::LimitChanged(s) => {
            app.history_limit_str = s.clone();
            let _ = app.db.lock().unwrap().set_setting("history_limit", &s);
            Task::none()
        }
        Message::Delete(id) => {
            let _ = app.db.lock().unwrap().delete_entry(id);
            app.update_search();
            Task::none()
        }
        Message::Obscure(id) => {
            app.obscuring_id = Some(id);
            app.obscuring_text = String::new();
            Task::none()
        }
        Message::ObscuringTextChanged(s) => {
            app.obscuring_text = s;
            Task::none()
        }
        Message::SaveObscure(id) => {
            let _ = app.db.lock().unwrap().set_obscure_label(id, &app.obscuring_text);
            app.obscuring_id = None;
            app.update_search();
            Task::none()
        }
        Message::CancelObscure => {
            app.obscuring_id = None;
            Task::none()
        }
        Message::RemoveObscure(id) => {
            let _ = app.db.lock().unwrap().set_obscure_label(id, "");
            app.update_search();
            Task::none()
        }
        Message::Copy(text) => {
            if let Ok(mut cb) = Clipboard::new() {
                let _ = cb.set_text(text);
            }
            *app.is_visible.lock().unwrap() = false;
            if let Some(id) = app.window_id {
                return iced::window::set_mode(id, iced::window::Mode::Hidden);
            }
            Task::none()
        }
        Message::TogglePin(id) => {
            let mut pin_state = false;
            if let Some(entry) = app.results.iter_mut().find(|e| e.id == id) {
                entry.is_pinned = !entry.is_pinned;
                pin_state = entry.is_pinned;
            }
            let _ = app.db.lock().unwrap().toggle_pin(id, pin_state);
            app.update_search();
            Task::none()
        }
        Message::RunOnStartup => {
            if let Ok(exe_path) = std::env::current_exe() {
                if let Some(autostart_dir) = dirs::config_dir().map(|d| d.join("autostart")) {
                    let _ = std::fs::create_dir_all(&autostart_dir);
                    let desktop_file = autostart_dir.join("semanticclipboard.desktop");
                    let content = format!("[Desktop Entry]\nType=Application\nExec={}\nHidden=false\nNoDisplay=false\nX-GNOME-Autostart-enabled=true\nName=Semantic Clipboard\nComment=AI Clipboard\n", exe_path.display());
                    let _ = std::fs::write(desktop_file, content);
                }
            }
            Task::none()
        }
        Message::Unfocused => {
            *app.is_visible.lock().unwrap() = false;
            if let Some(id) = app.window_id {
                return iced::window::set_mode(id, iced::window::Mode::Hidden);
            }
            Task::none()
        }
        Message::ToggleAlwaysOnTop => {
            app.always_on_top = !app.always_on_top;
            if let Some(id) = app.window_id {
                let level = if app.always_on_top { iced::window::Level::AlwaysOnTop } else { iced::window::Level::Normal };
                return iced::window::set_level(id, level);
            }
            Task::none()
        }
        Message::MinimizeWindow => {
            std::process::exit(0);
        }
    }
}

pub fn view(app: &SemanticClipboardApp) -> Element<'_, Message> {
    if !*app.is_visible.lock().unwrap() {
        return container(Space::new().width(Length::Fill).height(Length::Fill))
            .style(|_theme| container::Style {
                background: Some(Color::TRANSPARENT.into()),
                ..Default::default()
            })
            .into();
    }

    let search_bar = container(
        row![
            text("🔍").size(16).style(|_theme: &Theme| text::Style { color: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.5)) }),
            text_input("Type here to search...", &app.search_query)
                .on_input(Message::SearchChanged)
                .width(Length::Fill)
                .style(|theme: &Theme, _status| {
                    let text_color = if theme == &Theme::Dark { Color::WHITE } else { Color::BLACK };
                    let placeholder_color = if theme == &Theme::Dark { Color::from_rgba(1.0, 1.0, 1.0, 0.5) } else { Color::from_rgba(0.0, 0.0, 0.0, 0.5) };
                    text_input::Style {
                        background: iced::Background::Color(Color::TRANSPARENT),
                        border: iced::Border::default(),
                        icon: Color::TRANSPARENT,
                        placeholder: placeholder_color,
                        value: text_color,
                        selection: Color::from_rgba(0.3, 0.5, 1.0, 0.5),
                    }
                })
                .padding(0),
            button(svg(svg::Handle::from_memory(if app.always_on_top { include_bytes!("../assets/pinned.svg") as &[u8] } else { include_bytes!("../assets/pin.svg") as &[u8] }))
                    .width(Length::Fixed(16.0)).height(Length::Fixed(16.0)).style(|theme: &Theme, _| iced::widget::svg::Style { color: Some(if theme == &Theme::Dark { Color::WHITE } else { Color::BLACK }) }))
                .on_press(Message::ToggleAlwaysOnTop)
                .style(button::text)
                .padding(4),
            button(svg(svg::Handle::from_memory(include_bytes!("../assets/close.svg")))
                    .width(Length::Fixed(16.0)).height(Length::Fixed(16.0)).style(|theme: &Theme, _| iced::widget::svg::Style { color: Some(if theme == &Theme::Dark { Color::WHITE } else { Color::BLACK }) }))
                .on_press(Message::MinimizeWindow)
                .style(button::text)
                .padding(4)
        ]
        .spacing(10)
        .align_y(Alignment::Center)
    )
    .padding([8, 16])
    .style(|_theme| container::Style {
        background: Some(Color::from_rgba(0.2, 0.22, 0.25, 1.0).into()),
        border: iced::Border {
            color: Color::from_rgba(0.4, 0.6, 1.0, 0.5),
            width: 1.5,
            radius: 20.0.into(),
        },
        ..Default::default()
    });

    let mut entries_col = column![].spacing(8);

    let pinned_entries: Vec<_> = app.results.iter().filter(|e| e.is_pinned).collect();
    let unpinned_entries: Vec<_> = app.results.iter().filter(|e| !e.is_pinned).collect();

    let mut generate_item_ui = |entry: &ClipboardEntry| -> Element<'_, Message> {
        let is_obscuring = app.obscuring_id == Some(entry.id);
        
        let content_ui: Element<'_, Message> = if is_obscuring {
            row![
                text_input("Enter private label...", &app.obscuring_text)
                    .on_input(Message::ObscuringTextChanged)
                    .on_submit(Message::SaveObscure(entry.id)),
                button(text("Save").size(14)).on_press(Message::SaveObscure(entry.id)).style(button::primary).padding(6),
                button(text("Cancel").size(14)).on_press(Message::CancelObscure).style(button::secondary).padding(6)
            ].spacing(8).into()
        } else if let Some(label) = &entry.obscured_label {
            let char_count = entry.content.chars().count();
            let partial = if char_count > 4 {
                format!("{}***{}", &entry.content[0..2], &entry.content[entry.content.len()-2..])
            } else {
                "***".to_string()
            };
            
            container(
                column![
                    text(partial).size(15),
                    text(format!("🔒 {}", label)).size(11).style(|_theme| text::Style { color: Some(Color::from_rgba(0.6, 0.6, 0.6, 1.0)) })
                ].spacing(2)
            )
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Left)
            .padding(8)
            .into()
        } else {
            let lines: Vec<&str> = entry.content.lines().take(4).collect();
            let mut preview = lines.join("\n");
            if entry.content.lines().count() > 4 {
                preview.push_str("\n...");
            }
            container(text(preview).size(15))
                .width(Length::Fill)
                .align_x(iced::alignment::Horizontal::Left)
                .padding(8)
                .into()
        };
        
        let icon_style = |theme: &Theme, _status: iced::widget::svg::Status| iced::widget::svg::Style {
            color: Some(if theme == &Theme::Dark { Color::WHITE } else { Color::BLACK }),
        };

        let copy_icon = svg(svg::Handle::from_memory(include_bytes!("../assets/copy.svg")))
            .width(Length::Fixed(18.0)).height(Length::Fixed(18.0)).style(icon_style);
        
        let lock_icon_svg = if entry.obscured_label.is_some() {
            svg(svg::Handle::from_memory(include_bytes!("../assets/lock.svg"))).width(Length::Fixed(18.0)).height(Length::Fixed(18.0)).style(|theme: &Theme, _| iced::widget::svg::Style {
                color: Some(if theme == &Theme::Dark { Color::from_rgba(1.0, 1.0, 1.0, 0.2) } else { Color::from_rgba(0.0, 0.0, 0.0, 0.2) })
            })
        } else {
            svg(svg::Handle::from_memory(include_bytes!("../assets/lock.svg"))).width(Length::Fixed(18.0)).height(Length::Fixed(18.0)).style(icon_style)
        };
        let delete_icon = svg(svg::Handle::from_memory(include_bytes!("../assets/delete.svg")))
            .width(Length::Fixed(18.0)).height(Length::Fixed(18.0)).style(icon_style);
        let pin_icon = svg(svg::Handle::from_memory(
            if entry.is_pinned { include_bytes!("../assets/pinned.svg") as &[u8] } else { include_bytes!("../assets/pin.svg") as &[u8] }
        )).width(Length::Fixed(18.0)).height(Length::Fixed(18.0)).style(icon_style);

        let mut lock_btn = button(lock_icon_svg).style(button::text).padding(6);
        if entry.obscured_label.is_none() {
            lock_btn = lock_btn.on_press(Message::Obscure(entry.id));
        }

        container(
            row![
                button(content_ui).on_press(Message::Copy(entry.content.clone())).style(button::text).width(Length::Fill),
                lock_btn,
                button(pin_icon).on_press(Message::TogglePin(entry.id)).style(button::text).padding(6),
                button(copy_icon).on_press(Message::Copy(entry.content.clone())).style(button::text).padding(6),
                button(delete_icon).on_press(Message::Delete(entry.id)).style(button::text).padding(6),
            ]
            .spacing(4)
            .align_y(Alignment::Center)
        )
        .style(|theme: &Theme| container::Style {
            background: Some(if theme == &Theme::Dark { Color::from_rgba(0.15, 0.15, 0.15, 0.6).into() } else { Color::from_rgba(0.9, 0.9, 0.9, 0.6).into() }),
            border: iced::Border {
                color: if theme == &Theme::Dark { Color::from_rgba(1.0, 1.0, 1.0, 0.05) } else { Color::from_rgba(0.0, 0.0, 0.0, 0.05) },
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .padding(8)
        .into()
    };

    if !pinned_entries.is_empty() {
        for entry in pinned_entries {
            entries_col = entries_col.push(generate_item_ui(entry));
        }
        if !unpinned_entries.is_empty() {
            entries_col = entries_col.push(
                rule::horizontal(1).style(|theme: &Theme| rule::Style {
                    color: if theme == &Theme::Dark { Color::from_rgba(1.0, 1.0, 1.0, 0.2) } else { Color::from_rgba(0.0, 0.0, 0.0, 0.2) },
                    fill_mode: rule::FillMode::Full,
                    radius: 0.0.into(),
                    snap: true,
                })
            );
        }
    }
    for entry in unpinned_entries {
        entries_col = entries_col.push(generate_item_ui(entry));
    }

    let mut main_col = column![
        search_bar,
        scrollable(entries_col)
            .direction(iced::widget::scrollable::Direction::Vertical(
                iced::widget::scrollable::Scrollbar::new()
                    .width(0)
                    .scroller_width(0)
                    .margin(0)
            ))
            .height(Length::Fill),
    ]
    .spacing(20);

    if app.show_settings {
        let db_size_mb = app.db.lock().unwrap().get_db_size_bytes() as f64 / 1_048_576.0;

        let os_guide = if cfg!(target_os = "linux") {
            "Linux/Wayland: Global shortcuts are blocked by Wayland security. Bind your OS custom shortcut to run 'SemanticClipboard' to pop up this window. Double-tapping Ctrl+Alt only works on X11."
        } else if cfg!(target_os = "macos") {
            "macOS: Double-tap Ctrl+Alt. Alternatively, use Shortcuts/Automator to run 'SemanticClipboard'."
        } else {
            "Windows: Double-tap Ctrl+Alt to toggle the clipboard UI."
        };

        let guide_container = container(text(os_guide).size(13).style(|_theme| text::Style { color: Some(Color::from_rgba(0.6, 0.6, 0.6, 1.0)) }))
            .padding(12)
            .style(|theme: &Theme| container::Style {
                background: Some(if theme == &Theme::Dark { Color::from_rgba(1.0, 1.0, 1.0, 0.05).into() } else { Color::from_rgba(0.0, 0.0, 0.0, 0.05).into() }),
                border: iced::Border { radius: 8.0.into(), ..Default::default() },
                ..Default::default()
            });

        let settings_col = column![
            text("System Preferences").size(18),
            guide_container,
            rule::horizontal(1).style(|theme: &Theme| rule::Style {
                color: if theme == &Theme::Dark { Color::from_rgba(1.0, 1.0, 1.0, 0.1) } else { Color::from_rgba(0.0, 0.0, 0.0, 0.1) },
                fill_mode: rule::FillMode::Full,
                radius: 0.0.into(),
                snap: true,
            }),
            row![text("Unpinned History Limit:").width(200), text_input("", &app.history_limit_str).on_input(Message::LimitChanged)],
            row![
                text("Show in System Tray (Click outside to hide):").width(Length::Fill),
                toggler(app.show_in_tray).on_toggle(Message::ToggleShowInTray)
            ].align_y(Alignment::Center),
            text(format!("Cache File Size: {:.2} MB", db_size_mb)),
            button(text("Add to Startup").size(14)).on_press(Message::RunOnStartup).style(button::secondary).padding(6),
        ]
        .spacing(16)
        .padding(16);

        let styled_settings = container(settings_col)
            .width(Length::Fill)
            .style(|theme: &Theme| container::Style {
                background: Some(if theme == &Theme::Dark { Color::from_rgba(0.1, 0.1, 0.1, 0.5).into() } else { Color::from_rgba(0.9, 0.9, 0.9, 0.5).into() }),
                border: iced::Border { radius: 12.0.into(), ..Default::default() },
                ..Default::default()
            });

        main_col = main_col.push(styled_settings);
    }

    let footer = column![
        rule::horizontal(1).style(|_theme| rule::Style {
            color: Color::from_rgba(1.0, 1.0, 1.0, 0.1),
            fill_mode: rule::FillMode::Full,
            radius: 0.0.into(),
            snap: true,
        }),
        Space::new().height(Length::Fixed(4.0)),
        row![
            svg(svg::Handle::from_memory(include_bytes!("../assets/private.svg"))).width(Length::Fixed(18.0)).height(Length::Fixed(18.0)).style(|theme: &Theme, _| iced::widget::svg::Style { color: Some(if theme == &Theme::Dark { Color::WHITE } else { Color::BLACK }) }),
            text("Private mode").size(16),
            Space::new().width(Length::Fill),
            toggler(app.cache_only_pinned).on_toggle(Message::ToggleCachePinned)
        ].spacing(12).align_y(Alignment::Center),
        
        row![
            button(
                row![
                    svg(svg::Handle::from_memory(include_bytes!("../assets/settings.svg"))).width(Length::Fixed(18.0)).height(Length::Fixed(18.0)).style(|theme: &Theme, _| iced::widget::svg::Style { color: Some(if theme == &Theme::Dark { Color::WHITE } else { Color::BLACK }) }),
                    text("Settings").size(16)
                ].spacing(12).align_y(Alignment::Center)
            ).on_press(Message::ToggleSettings).style(button::text).padding(0),
            Space::new().width(Length::Fill)
        ].align_y(Alignment::Center),
        
        row![
            button(
                row![
                    svg(svg::Handle::from_memory(include_bytes!("../assets/delete.svg"))).width(Length::Fixed(18.0)).height(Length::Fixed(18.0)).style(|theme: &Theme, _| iced::widget::svg::Style { color: Some(Color::from_rgb(1.0, 0.4, 0.4)) }),
                    text("Clear history").size(16).style(|_theme: &Theme| text::Style { color: Some(Color::from_rgb(1.0, 0.4, 0.4)) })
                ].spacing(12).align_y(Alignment::Center)
            ).on_press(Message::ClearHistory).style(button::text).padding(0),
            Space::new().width(Length::Fill)
        ].align_y(Alignment::Center),
        
        row![
            button(
                row![
                    svg(svg::Handle::from_memory(include_bytes!("../assets/close.svg"))).width(Length::Fixed(18.0)).height(Length::Fixed(18.0)).style(|theme: &Theme, _| iced::widget::svg::Style { color: Some(if theme == &Theme::Dark { Color::WHITE } else { Color::BLACK }) }),
                    text("Close window").size(16)
                ].spacing(12).align_y(Alignment::Center)
            ).on_press(Message::MinimizeWindow).style(button::text).padding(0),
            Space::new().width(Length::Fill)
        ].align_y(Alignment::Center),
    ].spacing(16);

    main_col = main_col.push(footer);

    let drag_area = mouse_area(
        container(main_col)
            .padding(24)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|theme: &Theme| container::Style {
                text_color: Some(theme.palette().text),
                background: Some(if theme == &Theme::Dark {
                    Color::from_rgba(0.12, 0.12, 0.12, 0.85).into()
                } else {
                    Color::from_rgba(0.98, 0.98, 0.98, 0.85).into()
                }),
                border: iced::Border {
                    color: if theme == &Theme::Dark { Color::from_rgba(1.0, 1.0, 1.0, 0.15) } else { Color::from_rgba(0.0, 0.0, 0.0, 0.15) },
                    width: 1.0,
                    radius: 20.0.into(),
                },
                ..Default::default()
            })
    ).on_press(Message::DragWindow);

    drag_area.into()
}

pub fn subscription(_app: &SemanticClipboardApp) -> Subscription<Message> {
    Subscription::batch(vec![
        iced::time::every(std::time::Duration::from_millis(50)).map(|_| Message::Tick),
        iced::event::listen_with(|event, _status, id| {
            if let iced::Event::Window(w_event) = event {
                if w_event == iced::window::Event::Unfocused {
                    Some(Message::Unfocused)
                } else {
                    Some(Message::WindowId(id))
                }
            } else {
                None
            }
        })
    ])
}

pub fn theme(_app: &SemanticClipboardApp) -> Theme {
    match dark_light::detect() {
        Ok(dark_light::Mode::Dark) => Theme::Dark,
        Ok(dark_light::Mode::Light) => Theme::Light,
        _ => Theme::Dark,
    }
}
