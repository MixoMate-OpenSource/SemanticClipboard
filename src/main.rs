mod db;
mod ml;
mod clipboard;
mod ui;

use db::Database;
use ml::MLEngine;
use clipboard::ClipboardMonitor;
use ui::SemanticClipboardApp;
use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::{AtomicU8, AtomicBool, Ordering};
use iced::window;

pub fn main() -> iced::Result {
    println!("Semantic Clipboard starting...");

    let socket_addr = "127.0.0.1:45454";
    if let Ok(mut stream) = std::net::TcpStream::connect(socket_addr) {
        use std::io::Write;
        let _ = stream.write_all(b"WAKEUP");
        println!("Background instance already running. Sent WAKEUP signal to open UI.");
        return Ok(());
    }

    // Initialize Database
    let db = Arc::new(Mutex::new(Database::new().expect("Failed to initialize database")));
    
    // Setup model paths
    let data_dir = dirs::data_local_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let app_dir = data_dir.join("SemanticClipboard");
    let model_path = app_dir.join("model.onnx");
    let tokenizer_path = app_dir.join("tokenizer.json");

    // Ensure directory exists
    std::fs::create_dir_all(&app_dir).unwrap();

    // Download models with progress output
    let (model_path, tokenizer_path) = tokio::runtime::Runtime::new().unwrap().block_on(async {
        MLEngine::download_models_if_needed(|progress| {
            print!("\rDownloading model: {:.1}%", progress * 100.0);
            use std::io::Write;
            let _ = std::io::stdout().flush();
        }).await.expect("Failed to download models")
    });
    println!("\nModels ready.");

    // Initialize ML Engine
    let ml = Arc::new(Mutex::new(MLEngine::new(&model_path, &tokenizer_path).expect("Failed to init ML")));

    let is_visible = Arc::new(Mutex::new(true));
    let needs_refresh = Arc::new(AtomicBool::new(false));
    let command_flag = Arc::new(AtomicU8::new(0)); // 0: None, 1: Toggle, 2: Paste, 3: Clear



    // Spawn clipboard monitor
    let monitor = ClipboardMonitor::new(db.clone(), ml.clone(), needs_refresh.clone());
    monitor.spawn();

    // Spawn System Tray Thread
    std::thread::spawn(move || {
        #[cfg(target_os = "linux")]
        if let Err(e) = gtk::init() {
            eprintln!("Failed to initialize GTK for system tray: {:?}", e);
            return;
        }

        use tray_icon::{menu::{Menu, MenuItem, PredefinedMenuItem}, TrayIconBuilder, Icon};
        
        let menu = Menu::new();
        let show_i = MenuItem::new("Show Clipboard", true, None);
        let quit_i = MenuItem::new("Quit", true, None);
        let _ = menu.append_items(&[&show_i, &PredefinedMenuItem::separator(), &quit_i]);

        // Create a basic 16x16 icon for the tray
        let icon_rgba = include_bytes!("../icons/icon.png");
        let icon = match image::load_from_memory(icon_rgba) {
            Ok(img) => {
                let img = img.into_rgba8();
                let (width, height) = img.dimensions();
                Icon::from_rgba(img.into_raw(), width, height).ok()
            },
            Err(_) => None
        };

        let mut builder = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Semantic Clipboard");
        
        if let Some(i) = icon {
            builder = builder.with_icon(i);
        }
        
        let _tray_icon = builder.build().unwrap_or_else(|e| {
            eprintln!("Failed to build tray icon: {:?}", e);
            std::process::exit(1);
        });

        let menu_channel = tray_icon::menu::MenuEvent::receiver();

        #[cfg(target_os = "linux")]
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            if let Ok(event) = menu_channel.try_recv() {
                if event.id == show_i.id() {
                    use std::io::Write;
                    let _ = std::net::TcpStream::connect("127.0.0.1:45454").and_then(|mut s| s.write_all(b"WAKEUP"));
                } else if event.id == quit_i.id() {
                    std::process::exit(0);
                }
            }
            gtk::glib::ControlFlow::Continue
        });

        #[cfg(target_os = "linux")]
        gtk::main();
        
        #[cfg(not(target_os = "linux"))]
        loop {
            if let Ok(event) = menu_channel.try_recv() {
                if event.id == show_i.id() {
                    use std::io::Write;
                    let _ = std::net::TcpStream::connect("127.0.0.1:45454").and_then(|mut s| s.write_all(b"WAKEUP"));
                } else if event.id == quit_i.id() {
                    std::process::exit(0);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });

    let cmd_flag_tcp = command_flag.clone();
    std::thread::spawn(move || {
        if let Ok(listener) = std::net::TcpListener::bind(socket_addr) {
            for stream in listener.incoming() {
                if let Ok(_) = stream {
                    // Received connection, show UI
                    cmd_flag_tcp.store(4, Ordering::SeqCst);
                }
            }
        }
    });

    let cmd_flag_hotkey = command_flag.clone();

    // Spawn global hotkey listener
    std::thread::spawn(move || {
        use rdev::{listen, Event, EventType, Key};
        let mut ctrl_pressed = false;
        let mut alt_pressed = false;
        let mut both_were_pressed = false;
        let mut last_activation = std::time::Instant::now() - std::time::Duration::from_secs(10);

        let callback = move |event: Event| {
            let mut changed = false;
            match event.event_type {
                EventType::KeyPress(Key::ControlLeft) | EventType::KeyPress(Key::ControlRight) => { ctrl_pressed = true; changed = true; },
                EventType::KeyRelease(Key::ControlLeft) | EventType::KeyRelease(Key::ControlRight) => { ctrl_pressed = false; changed = true; },
                EventType::KeyPress(Key::Alt) | EventType::KeyPress(Key::AltGr) => { alt_pressed = true; changed = true; },
                EventType::KeyRelease(Key::Alt) | EventType::KeyRelease(Key::AltGr) => { alt_pressed = false; changed = true; },
                _ => {}
            }

            if changed {
                let both_pressed = ctrl_pressed && alt_pressed;
                if both_pressed && !both_were_pressed {
                    let now = std::time::Instant::now();
                    if now.duration_since(last_activation) < std::time::Duration::from_millis(1000) {
                        cmd_flag_hotkey.store(1, Ordering::SeqCst);
                        // Reset to prevent repeated triggering
                        last_activation = now - std::time::Duration::from_secs(10);
                    } else {
                        last_activation = now;
                    }
                }
                both_were_pressed = both_pressed;
            }
        };

        if let Err(error) = listen(callback) {
            eprintln!("Error listening to hotkeys: {:?}", error);
        }
    });

    iced::application(
        move || (SemanticClipboardApp::new(db.clone(), ml.clone(), is_visible.clone(), needs_refresh.clone(), command_flag.clone()), iced::Task::none()),
        ui::update,
        ui::view
    )
    .title("Semantic Clipboard")
    .style(|_state, _theme| iced::theme::Style {
        background_color: iced::Color::TRANSPARENT,
        text_color: iced::Color::WHITE,
    })
    .window(window::Settings {
        transparent: true,
        decorations: false,
        level: window::Level::AlwaysOnTop,
        ..Default::default()
    })
    .subscription(ui::subscription)
    .theme(ui::theme)
    .run()
}
