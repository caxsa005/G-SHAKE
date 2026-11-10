
#![windows_subsystem = "windows"]

use rdev::{listen, Event, EventType};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tao::{
    dpi::{LogicalPosition, LogicalSize},
    event::{Event as TaoEvent, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    window::{Window, WindowBuilder},
};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    TrayIconBuilder,
};
use wry::{WebView, WebViewBuilder};

// Custom events sent from our background threads to the main GUI thread
#[derive(Debug)]
enum AppEvent {
    ShakeTriggered { x: i32, y: i32 },
    CursorLeftBounds,
    FadeOutStep(f32),
}

fn main() {
    // 1. Initialize Event Loop and inter-thread proxy
    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let proxy_clone = proxy.clone();

    // 2. Setup System Tray Menu
    let tray_menu = Menu::new().unwrap();
    let settings_item = MenuItem::new("G-SHAKE Settings", true, None);
    let quit_item = MenuItem::new("Quit G-SHAKE", true, None);
    tray_menu.append(&settings_item).unwrap();
    tray_menu.append(&quit_item).unwrap();

    let _tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("G-SHAKE Background Service")
        .build()
        .unwrap();

    let menu_channel = MenuEvent::receiver();

    // 3. Application State
    let mut overlay_window: Option<(Window, WebView)> = None;
    let mut settings_window: Option<(Window, WebView)> = None;
    
    // Thread-safe state to track if the overlay is currently visible to the user
    let is_overlay_visible = Arc::new(Mutex::new(false));
    let is_overlay_visible_listener = Arc::clone(&is_overlay_visible);
    
    // Window bounding box state for auto-dismiss
    let window_bounds = Arc::new(Mutex::new((0, 0))); // (X, Y)
    let window_bounds_listener = Arc::clone(&window_bounds);

    // 4. Background Mouse Listener Thread
    thread::spawn(move || {
        let mut last_time = Instant::now();
        let mut last_x = 0.0;
        let mut shake_count = 0;
        let mut last_direction = 0;

        let callback = move |event: Event| {
            if let EventType::MouseMove { x, y } = event.event_type {
                let is_visible = *is_overlay_visible_listener.lock().unwrap();
                
                // If overlay is open, check if cursor left the bounds
                if is_visible {
                    let (wx, wy) = *window_bounds_listener.lock().unwrap();
                    let left = wx - 20;
                    let right = wx + 450 + 20;
                    let top = wy - 20;
                    let bottom = wy + 600 + 20;

                    if (x as i32) < left || (x as i32) > right || (y as i32) < top || (y as i32) > bottom {
                        let _ = proxy_clone.send_event(AppEvent::CursorLeftBounds);
                    }
                }

                // Shake Detection Logic
                let current_time = Instant::now();
                let delta_x = x - last_x;
                let time_diff = current_time.duration_since(last_time).as_secs_f32();

                if time_diff > 0.25 {
                    shake_count = 0;
                } else if delta_x.abs() > 20.0 {
                    let direction = if delta_x > 0.0 { 1 } else { -1 };
                    if last_direction != 0 && direction != last_direction {
                        shake_count += 1;
                    }
                    last_direction = direction;

                    // Trigger at 4 directional reversals
                    if shake_count >= 4 && !is_visible {
                        let _ = proxy_clone.send_event(AppEvent::ShakeTriggered { 
                            x: x as i32, 
                            y: y as i32 
                        });
                        shake_count = 0;
                    }
                }
                last_x = x;
                last_time = current_time;
            }
        };

        if let Err(e) = listen(callback) {
            eprintln!("Mouse listener crashed: {:?}", e);
        }
    });

    // Determine the persistent data directory for Gemini logins (AppData/Local/G-SHAKE)
    let data_dir = std::env::temp_dir().join("G_SHAKE_UserData");

    // CSS injected into Gemini to create the blue moving blurred overlay
    let gemini_css_injection = r#"
        const style = document.createElement('style');
        style.innerHTML = `
            body { 
                background: transparent !important; 
            }
            .gemini-bg-animation {
                position: fixed;
                top: 0; left: 0; width: 100vw; height: 100vh;
                z-index: -9999;
                background: linear-gradient(135deg, rgba(15, 32, 60, 0.85), rgba(0, 80, 140, 0.7), rgba(10, 40, 90, 0.8));
                background-size: 200% 200%;
                animation: smoothGradient 8s ease infinite;
                backdrop-filter: blur(15px);
            }
            @keyframes smoothGradient {
                0% { background-position: 0% 50%; }
                50% { background-position: 100% 50%; }
                100% { background-position: 0% 50%; }
            }
        `;
        document.head.appendChild(style);
        
        const animatedBg = document.createElement('div');
        animatedBg.className = 'gemini-bg-animation';
        document.body.prepend(animatedBg);
    "#;

    // AMOLED Black Settings UI
    let settings_html = r#"
        <!DOCTYPE html>
        <html lang="en">
        <head>
            <meta charset="UTF-8">
            <style>
                body {
                    background-color: #000000; /* AMOLED Black */
                    color: #FFFFFF;
                    font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif;
                    display: flex;
                    flex-direction: column;
                    align-items: center;
                    justify-content: center;
                    height: 100vh;
                    margin: 0;
                }
                h1 { color: #0078D7; letter-spacing: 2px; }
                .card {
                    background: #0A0A0A;
                    border: 1px solid #1A1A1A;
                    padding: 20px;
                    border-radius: 12px;
                    width: 80%;
                    text-align: center;
                }
                button {
                    background: #0078D7; color: white;
                    border: none; padding: 10px 20px; border-radius: 6px;
                    cursor: pointer; margin-top: 20px;
                }
                button:hover { background: #005A9E; }
            </style>
        </head>
        <body>
            <h1>G-SHAKE</h1>
            <div class="card">
                <p>Background Engine: <strong>Active</strong></p>
                <p>Shake your cursor back and forth to summon Gemini.</p>
                <button onclick="alert('Settings saved!')">Save Preferences</button>
            </div>
        </body>
        </html>
    "#;

    // 5. Main UI Event Loop
    event_loop.run(move |event, event_loop_window_target, control_flow| {
        *control_flow = ControlFlow::Wait;

        // Process Tray Clicks
        if let Ok(tray_event) = menu_channel.try_recv() {
            if tray_event.id == quit_item.id() {
                *control_flow = ControlFlow::Exit;
            } else if tray_event.id == settings_item.id() {
                if settings_window.is_none() {
                    let window = WindowBuilder::new()
                        .with_title("G-SHAKE Settings")
                        .with_inner_size(LogicalSize::new(450.0, 400.0))
                        .with_resizable(false)
                        .build(event_loop_window_target)
                        .unwrap();

                    let webview = WebViewBuilder::new()
                        .with_html(settings_html)
                        .build(&window)
                        .unwrap();

                    settings_window = Some((window, webview));
                }
            }
        }

        match event {
            // Handle Custom Background Events
            TaoEvent::UserEvent(app_event) => match app_event {
                AppEvent::ShakeTriggered { x, y } => {
                    let pos_x = (x as f64 - 225.0).max(0.0);
                    let pos_y = (y as f64 - 50.0).max(0.0);
                    
                    *window_bounds.lock().unwrap() = (pos_x as i32, pos_y as i32);
                    *is_overlay_visible.lock().unwrap() = true;

                    if let Some((ref window, _)) = overlay_window {
                        window.set_outer_position(LogicalPosition::new(pos_x, pos_y));
                        //window.set_opacity(1.0);
                        window.set_visible(true);
                        //window.focus_window();
                    } else {
                        // Create the exact Google Books style floating window
                        let window = WindowBuilder::new()
                            .with_decorations(false)
                            .with_transparent(true)
                            .with_always_on_top(true)
                            .with_inner_size(LogicalSize::new(450.0, 600.0))
                            .with_position(LogicalPosition::new(pos_x, pos_y))
                            .build(event_loop_window_target)
                            .unwrap();

                        // Create the WebView connecting to Gemini with login persistence
                        let webview = WebViewBuilder::new(&window)
                            .with_transparent(true)
                            .with_data_directory(Some(data_dir.clone())) // Retains Google Logins
                            .with_initialization_script(gemini_css_injection) // Applies the blue moving blur
                            .with_url("https://gemini.google.com")
                            .build(&window)
                            .unwrap();

                        let webview = WebViewBuilder::new(&window)
    .with_transparent(true)
    .build()?;
                    }
                }
                AppEvent::CursorLeftBounds => {
                    // Trigger fade out
                    *is_overlay_visible.lock().unwrap() = false;
                    let p = proxy.clone();
                    thread::spawn(move || {
                        for i in (0..=10).rev() {
                            let _ = p.send_event(AppEvent::FadeOutStep(i as f32 / 10.0));
                            thread::sleep(Duration::from_millis(15));
                        }
                    });
                }
                AppEvent::FadeOutStep(alpha) => {
                    if let Some((ref window, _)) = overlay_window {
                        //window.set_opacity(alpha);
                        if alpha <= 0.0 {
                            window.set_visible(false);
                        }
                    }
                }
            },
            
            // Clean up windows when X is clicked
            TaoEvent::WindowEvent { event: WindowEvent::CloseRequested, window_id, .. } => {
                if let Some((ref win, _)) = settings_window {
                    if win.id() == window_id {
                        settings_window = None;
                    }
                }
            }
            _ => (),
        }
    });
}