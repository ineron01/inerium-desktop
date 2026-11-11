use std::cell::RefCell;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;

use gdk4::prelude::*;
use gdk4::Key;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, CssProvider, EventControllerKey};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use serde_json::Value;

mod hw;
mod theme;
mod views;

unsafe fn libc_getuid() -> u32 {
    unsafe extern "C" {
        fn getuid() -> u32;
    }
    unsafe { getuid() }
}

fn get_sock_path() -> PathBuf {
    let uid = unsafe { libc_getuid() };
    PathBuf::from(format!("/run/user/{}/kanagawa-popups.sock", uid))
}

fn get_hypr_socket() -> Option<PathBuf> {
    let rt = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc_getuid() }));
    let hypr_dir = PathBuf::from(&rt).join("hypr");

    let sig = env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .ok()
        .filter(|s| hypr_dir.join(s).join(".socket.sock").exists() || hypr_dir.join(s).join(".socket.real.sock").exists())
        .or_else(|| {
        if let Ok(entries) = fs::read_dir(&hypr_dir) {
            let mut subdirs: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .collect();
            subdirs.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
            subdirs.last().map(|e| e.file_name().to_string_lossy().into_owned())
        } else {
            None
        }
    })?;

    let base = hypr_dir.join(sig);
    if base.join(".socket.real.sock").exists() {
        Some(base.join(".socket.real.sock"))
    } else {
        Some(base.join(".socket.sock"))
    }
}

fn get_cursor_pos() -> (i32, i32) {
    if let Some(sock_path) = get_hypr_socket()
        && let Ok(mut stream) = UnixStream::connect(&sock_path) {
            let _ = stream.write_all(b"j/cursorpos");
            let mut buf = Vec::new();
            let _ = stream.read_to_end(&mut buf);
            if let Ok(v) = serde_json::from_slice::<Value>(&buf)
                && let (Some(x), Some(y)) = (v["x"].as_i64(), v["y"].as_i64()) {
                    return (x as i32, y as i32);
                }
        }
    (0, 0)
}

fn get_cursor_and_monitor() -> (i32, i32, i32, i32, i32, i32, String) {
    let mut cx = 960;
    let mut cy = 540;
    let mut mx = 0;
    let mut my = 0;
    let mut mw = 1920;
    let mut mh = 1080;
    let mut m_name = String::new();

    if let Some(sock_path) = get_hypr_socket()
        && let Ok(mut stream) = UnixStream::connect(&sock_path) {
            let _ = stream.write_all(b"[[BATCH]]j/cursorpos ; j/monitors");
            let mut buf = Vec::new();
            let _ = stream.read_to_end(&mut buf);
            let mut de = serde_json::Deserializer::from_slice(&buf).into_iter::<Value>();
            if let Some(Ok(c_val)) = de.next()
                && let (Some(x), Some(y)) = (c_val["x"].as_i64(), c_val["y"].as_i64()) {
                    cx = x as i32;
                    cy = y as i32;
                }
            if let Some(Ok(m_val)) = de.next()
                && let Some(list) = m_val.as_array() {
                    for mon in list {
                        let x = mon["x"].as_i64().unwrap_or(0) as i32;
                        let y = mon["y"].as_i64().unwrap_or(0) as i32;
                        let scale = mon["scale"].as_f64().unwrap_or(1.0);
                        let w = (mon["width"].as_f64().unwrap_or(1920.0) / scale).round() as i32;
                        let h = (mon["height"].as_f64().unwrap_or(1080.0) / scale).round() as i32;
                        let name = mon["name"].as_str().unwrap_or("").to_string();

                        if cx >= x && cx < x + w && cy >= y && cy < y + h {
                            mx = x;
                            my = y;
                            mw = w;
                            mh = h;
                            m_name = name;
                            break;
                        }
                    }
                }
        }

    (cx, cy, mx, my, mw, mh, m_name)
}

struct SafeZone {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    pad: i32,
    opened_at: std::time::Instant,
    has_entered: bool,
}

struct AppState {
    app: Option<Application>,
    window: Option<ApplicationWindow>,
    current_view: String,
    is_visible: bool,
    safe_zone: Option<SafeZone>,
    session_id: u64,
}

thread_local! {
    static STATE: RefCell<AppState> = const { RefCell::new(AppState {
        app: None,
        window: None,
        current_view: String::new(),
        is_visible: false,
        safe_zone: None,
        session_id: 0,
    }) };
}

static CMD_QUEUE: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn main() {
    let args: Vec<String> = env::args().collect();

    // Client mode: if passed a command and not --daemon, send to daemon socket and exit
    if args.len() > 1 && args[1] != "--daemon" {
        let sock = get_sock_path();
        if let Ok(mut stream) = UnixStream::connect(&sock) {
            let msg = format!("{}\n", args[1..].join(" "));
            let _ = stream.write_all(msg.as_bytes());
            let mut buf = [0u8; 16];
            let _ = stream.read(&mut buf);
            return;
        }
    }

    // Daemon mode: start GTK layer-shell application
    // Isolate shell styling from external GTK_THEME overrides
    unsafe { env::remove_var("GTK_THEME"); }

    let app = Application::builder()
        .application_id("org.kanagawa.shell")
        .build();

    app.connect_startup(move |app| {
        if let Some(settings) = gtk4::Settings::default() {
            settings.set_gtk_application_prefer_dark_theme(true);
        }
        // Load global Kanagawa CSS with priority above USER to guarantee pristine Dragon styling
        let provider = CssProvider::new();
        provider.load_from_data(theme::POPUP_CSS);
        if let Some(display) = gdk4::Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_USER + 100,
            );
        }

        STATE.with(|s| {
            s.borrow_mut().app = Some(app.clone());
        });
    });

    app.connect_activate(move |app| {
        let guard = app.hold();
        Box::leak(Box::new(guard)); // Keep daemon held permanently

        let sock_path = get_sock_path();
        if sock_path.exists() {
            let _ = fs::remove_file(&sock_path);
        }

        // Start Unix domain socket listener thread
        thread::spawn(move || {
            let listener = match UnixListener::bind(&sock_path) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Failed to bind socket: {}", e);
                    return;
                }
            };

            for mut s in listener.incoming().flatten() {
                let mut reader = BufReader::new(&mut s);
                let mut line = String::new();
                if reader.read_line(&mut line).is_ok() {
                    let cmd = line.trim().to_string();
                    if let Ok(mut q) = CMD_QUEUE.lock() {
                        q.push(cmd);
                    }

                    glib::MainContext::default().invoke(|| {
                        let cmds: Vec<String> = if let Ok(mut q) = CMD_QUEUE.lock() {
                            q.drain(..).collect()
                        } else {
                            Vec::new()
                        };

                        for c in cmds {
                            handle_command(&c);
                        }
                    });

                    let _ = s.write_all(b"OK\n");
                }
            }
        });
    });

    app.run_with_args(&["kanagawa-shell"]);
}

fn handle_command(cmd: &str) {
    STATE.with(|s| {
        let mut state = s.borrow_mut();
        let is_vis = state.is_visible;
        let cur_view = state.current_view.clone();

        let parts: Vec<&str> = cmd.split_whitespace().collect();
        let target = parts.first().copied().unwrap_or("control").to_lowercase();

        if target == "close" {
            hide_window(&mut state);
            return;
        }

        if is_vis && cur_view == target {
            hide_window(&mut state);
            return;
        }

        state.current_view = target.clone();
        show_view(&mut state, &target);
    });
}

fn hide_window(state: &mut AppState) {
    if let Some(win) = state.window.take() {
        win.close();
    }
    state.is_visible = false;
    state.current_view.clear();
    state.safe_zone = None;
}

fn show_view(state: &mut AppState, view_name: &str) {
    if let Some(old_win) = state.window.take() {
        old_win.close();
    }

    let app = match state.app.as_ref() {
        Some(a) => a.clone(),
        None => return,
    };

    let (cx, cy, mx, my, mw, mh, m_name) = get_cursor_and_monitor();
    eprintln!("show_view: view={}, m_name={}, cx={}, cy={}, mx={}, my={}, mw={}, mh={}", view_name, m_name, cx, cy, mx, my, mw, mh);

    let win = ApplicationWindow::builder()
        .application(&app)
        .css_classes(["popup-window"])
        .build();

    win.init_layer_shell();
    if view_name == "power" {
        win.set_namespace(Some("kanagawa-power"));
        win.add_css_class("power-window");
    } else {
        win.set_namespace(Some("org.kanagawa.popups.daemon"));
    }
    win.set_layer(Layer::Overlay);

    // Assign window to the active GdkMonitor
    if let Some(display) = gdk4::Display::default() {
        let gdk_mons = display.monitors();
        for i in 0..gdk_mons.n_items() {
            if let Some(m) = gdk_mons.item(i).and_downcast::<gdk4::Monitor>()
                && m.connector().as_deref() == Some(&m_name) {
                    win.set_monitor(Some(&m));
                    break;
                }
        }
    }

    // Attach Escape key controller
    let key_ctrl = EventControllerKey::new();
    key_ctrl.connect_key_pressed(|_, key, _, _| {
        if key == Key::Escape {
            STATE.with(|s| {
                let mut st = s.borrow_mut();
                hide_window(&mut st);
            });
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    win.add_controller(key_ctrl);

    let nav_cb = move |next_view: &str| {
        let n = next_view.to_string();
        STATE.with(|s| {
            let mut st = s.borrow_mut();
            st.current_view = n.clone();
            show_view(&mut st, &n);
        });
    };

    let close_cb = move || {
        STATE.with(|s| {
            let mut st = s.borrow_mut();
            hide_window(&mut st);
        });
    };

    let (child, req_w, req_h) = match view_name {
        "audio" => views::audio::build_audio_view(nav_cb, close_cb),
        "bluetooth" => views::bluetooth::build_bluetooth_view(nav_cb, close_cb),
        "network" => views::network::build_network_view(nav_cb, close_cb),
        "media" => views::media::build_media_view(nav_cb, close_cb),
        "power" => views::power::build_power_menu(close_cb),
        "wallpaper" => views::wallpaper::build_wallpaper_view(nav_cb, close_cb, Some(m_name)),
        _ => views::control_center::build_control_center(nav_cb, close_cb),
    };

    let max_h = mh - 44;
    let effective_h = req_h.min(max_h);

    let (win_global_x, win_global_y) = if view_name == "power" {
        win.set_keyboard_mode(KeyboardMode::Exclusive);
        win.set_exclusive_zone(-1);
        win.set_anchor(Edge::Top, true);
        win.set_anchor(Edge::Bottom, true);
        win.set_anchor(Edge::Left, true);
        win.set_anchor(Edge::Right, true);
        win.set_child(Some(&child));
        (mx, my)
    } else {
        win.set_size_request(req_w, effective_h);
        win.set_child(Some(&child));

        win.set_keyboard_mode(KeyboardMode::OnDemand);
        let rel_x = cx - mx;
        let max_margin_left = (mw - req_w - 8).max(8);
        let mut margin_left = rel_x - (req_w / 2);
        margin_left = margin_left.clamp(8, max_margin_left);

        win.set_anchor(Edge::Top, true);
        win.set_anchor(Edge::Left, true);
        win.set_anchor(Edge::Bottom, false);
        win.set_anchor(Edge::Right, false);
        win.set_margin(Edge::Top, 4);
        win.set_margin(Edge::Left, margin_left);
        (mx + margin_left, my + 40)
    };

    let pad = 65;
    let x_min_init = win_global_x - pad;
    let x_max_init = win_global_x + req_w + pad;
    let y_min_init = my;
    let y_max_init = win_global_y + effective_h + pad;
    let init_inside = cx >= x_min_init && cx <= x_max_init && cy >= y_min_init && cy <= y_max_init;

    state.session_id = state.session_id.wrapping_add(1);
    let current_session = state.session_id;

    state.safe_zone = Some(SafeZone {
        x: win_global_x,
        y: win_global_y,
        w: req_w,
        h: effective_h,
        pad,
        opened_at: std::time::Instant::now(),
        has_entered: init_inside,
    });

    win.present();
    state.window = Some(win);
    state.is_visible = true;

    // Start Safe Zone watcher timer (runs only while is_visible and session matches)
    if view_name != "power" {
        glib::timeout_add_local(std::time::Duration::from_millis(80), move || {
            STATE.with(|s| {
                let mut st = s.borrow_mut();
                if !st.is_visible || st.session_id != current_session {
                    return glib::ControlFlow::Break;
                }

                let (alloc_w, alloc_h) = if let Some(ref w) = st.window {
                    (w.allocated_width(), w.allocated_height())
                } else {
                    (0, 0)
                };

                if let Some(ref mut sz) = st.safe_zone {
                    if alloc_w > 50 {
                        sz.w = alloc_w;
                    }
                    if alloc_h > 50 {
                        sz.h = alloc_h;
                    }

                    let (cx, cy) = get_cursor_pos();
                    let x_min = sz.x - sz.pad;
                    let x_max = sz.x + sz.w + sz.pad;
                    let y_min = sz.y - sz.pad - 40; // include top bar area
                    let y_max = sz.y + sz.h + sz.pad;

                    let is_inside = cx >= x_min && cx <= x_max && cy >= y_min && cy <= y_max;

                    if is_inside {
                        sz.has_entered = true;
                    } else if sz.has_entered && sz.opened_at.elapsed() > std::time::Duration::from_millis(200) {
                        eprintln!("SafeZone closed: cx={}, cy={}, bounds=[{}, {}]x[{}, {}]", cx, cy, x_min, x_max, y_min, y_max);
                        hide_window(&mut st);
                        return glib::ControlFlow::Break;
                    }
                }

                glib::ControlFlow::Continue
            })
        });
    }
}
