mod config;
mod greetd;
mod selector;
mod sessions;

use std::cell::RefCell;
use std::rc::Rc;
use std::process::Command;

use gdk4::prelude::*;
use gtk4::prelude::*;
use gtk4::{glib, pango};

use config::{load_config, load_state, save_state, Config, State, DEFAULT_CONFIG_PATH, DEFAULT_STATE_PATH, DEMO_STATE_PATH};
use selector::SessionSelector;
use sessions::{read_sessions_filtered, Session};

const CSS: &str = r#"
* { font-family: "JetBrainsMono Nerd Font", monospace; color: #c5c9c5; }
window { background-color: #181616; }

.clock { font-size: 76px; font-weight: 800; color: #e6e2cc; }
.date  { font-size: 15px; font-weight: 700; color: #c4b28a; }

.card {
    background-color: rgba(24, 22, 22, 0.92);
    border: 1px solid rgba(197, 201, 197, 0.10);
    border-radius: 22px;
    padding: 24px;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.55);
}
.user { font-size: 15px; font-weight: 700; color: #e6e2cc; }
.user-sub { font-size: 12px; color: #737c73; }
.avatar {
    min-width: 48px; min-height: 48px; border-radius: 14px;
    background-color: rgba(135, 169, 135, 0.18); color: #87a987; font-size: 22px;
}

passwordentry {
    min-height: 48px; padding: 0 16px; border-radius: 12px;
    background-color: #221f1f; color: #e6e2cc; font-size: 14px;
    border: 1px solid rgba(197, 201, 197, 0.12); box-shadow: none; outline: none;
}
passwordentry:focus-within { border-color: rgba(135, 169, 135, 0.65); }
passwordentry.error { border-color: rgba(196, 116, 110, 0.85); }
passwordentry > text > placeholder { color: #737c73; }

.session-btn {
    min-height: 42px; padding: 0 14px; border-radius: 12px;
    background-color: #221f1f; background-image: none; box-shadow: none;
    border: 1px solid rgba(197, 201, 197, 0.08);
}
.session-btn:hover { border-color: rgba(197, 201, 197, 0.22); }
.session-btn .arrow { color: #737c73; font-size: 14px; }

.session-menu {
    background-color: rgba(20, 18, 18, 0.98);
    border: 1px solid rgba(197, 201, 197, 0.10);
    border-radius: 12px; padding: 4px;
    margin-top: 4px;
}
.session-item {
    min-height: 38px; padding: 0 12px; border-radius: 8px;
    background: transparent; border: none; box-shadow: none;
}
.session-item label { color: #c5c9c5; font-size: 13.5px; }
.session-item .check { color: #87a987; font-size: 14px; font-weight: bold; }
.session-item:hover {
    background-color: rgba(135, 169, 135, 0.14);
}
.session-item:hover label { color: #e6e2cc; }
.session-item.selected {
    background-color: rgba(135, 169, 135, 0.20);
}
.session-item.selected label {
    color: #87a987;
    font-weight: 700;
}

.go {
    min-width: 48px; min-height: 48px; padding: 0; border-radius: 12px;
    background-color: #87a987; background-image: none; border: none; box-shadow: none;
    color: #181616; font-size: 20px;
}
.go label { color: #181616; }
.go:hover { background-color: #98ba98; }
.go:disabled { opacity: 0.6; }

.label { font-size: 10.5px; font-weight: 700; color: #737c73; letter-spacing: 1px; }
.status { font-size: 11.5px; color: #737c73; }
.status.error { color: #c4746e; }
.status.warn { color: #c4b28a; }

.end {
    min-height: 34px; padding: 0 14px; border-radius: 12px; box-shadow: none; background-image: none;
    background-color: rgba(24, 22, 22, 0.85);
    border: 1px solid rgba(196, 116, 110, 0.35);
    font-size: 12px; font-weight: 600;
}
.end label { color: #c4746e; }
.end:hover { background-color: #c4746e; }
.end:hover label { color: #181616; }
"#;

#[derive(Clone, Copy, PartialEq, Eq)]
enum StatusKind {
    Normal,
    Warn,
    Error,
}

thread_local! {
    static GLOBAL_STATE: RefCell<Option<Rc<RefCell<GreeterState>>>> = const { RefCell::new(None) };
    static GLOBAL_APP: RefCell<Option<gtk4::Application>> = const { RefCell::new(None) };
}

struct GreeterState {
    demo: bool,
    _cfg: Config,
    _state: State,
    state_path: &'static str,
    user: String,
    sessions: Vec<Session>,
    busy: bool,
    has_error: bool,
    syncing_pw: bool,
    syncing_sess: bool,

    pws: Vec<gtk4::PasswordEntry>,
    gos: Vec<gtk4::Button>,
    sesses: Vec<SessionSelector>,
    statuses: Vec<gtk4::Label>,
    clocks: Vec<gtk4::Label>,
    dates: Vec<gtk4::Label>,
}

impl GreeterState {
    fn set_status(&self, text: &str, kind: StatusKind) {
        for st in &self.statuses {
            st.set_label(text);
            st.set_visible(!text.is_empty());
            st.remove_css_class("error");
            st.remove_css_class("warn");
            match kind {
                StatusKind::Error => st.add_css_class("error"),
                StatusKind::Warn => st.add_css_class("warn"),
                StatusKind::Normal => {}
            }
        }
    }

    fn set_busy(&mut self, busy: bool) {
        self.busy = busy;
        for pw in &self.pws {
            pw.set_sensitive(!busy);
        }
        for go in &self.gos {
            go.set_sensitive(!busy);
        }
        for s in &self.sesses {
            s.set_sensitive(!busy);
        }
    }

    fn get_password(&self) -> String {
        for pw in &self.pws {
            let t = pw.text().to_string();
            if !t.is_empty() {
                return t;
            }
        }
        String::new()
    }

    fn focus_pw(&self) {
        for pw in &self.pws {
            if pw.has_focus() {
                return;
            }
        }
        if let Some(first) = self.pws.first() {
            first.grab_focus();
        }
    }

    fn update_caps(&self) {
        if self.busy || self.has_error {
            return;
        }

        let is_caps = check_caps_lock();
        if is_caps {
            self.set_status("󰘲  Включён Caps Lock", StatusKind::Warn);
        } else {
            self.set_status("", StatusKind::Normal);
        }
    }
}

fn check_caps_lock() -> bool {
    if let Some(display) = gdk4::Display::default()
        && let Some(seat) = display.default_seat()
        && let Some(kb) = seat.keyboard()
    {
        return kb.is_caps_locked();
    }
    false
}

fn format_datetime() -> (String, String) {
    let now = glib::DateTime::now_local().unwrap_or_else(|_| glib::DateTime::from_unix_local(0).unwrap());
    let time_str = format!("{:02}:{:02}", now.hour(), now.minute());

    let day_idx = match now.day_of_week() {
        1 => 0, // Понедельник
        2 => 1,
        3 => 2,
        4 => 3,
        5 => 4,
        6 => 5,
        7 => 6, // Воскресенье
        _ => 0,
    };
    let month_idx = (now.month() - 1).clamp(0, 11) as usize;

    const DAYS: [&str; 7] = ["Понедельник", "Вторник", "Среда", "Четверг", "Пятница", "Суббота", "Воскресенье"];
    const MONTHS: [&str; 12] = [
        "января", "февраля", "марта", "апреля", "мая", "июня",
        "июля", "августа", "сентября", "октября", "ноября", "декабря"
    ];

    let date_str = format!("{}, {} {}", DAYS[day_idx], now.day_of_month(), MONTHS[month_idx]);
    (time_str, date_str)
}

fn system_action(cmd: &[&str], demo: bool) {
    if demo {
        println!("DEMO system action: {:?}", cmd);
        return;
    }
    if let Some((first, rest)) = cmd.split_first() {
        let _ = Command::new(first).args(rest).spawn();
    }
}

fn build_ui(app: &gtk4::Application, demo: bool) {
    // 1. Применяем глобальный CSS
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(CSS);
    if let Some(disp) = gdk4::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &disp,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    if let Some(settings) = gtk4::Settings::default() {
        settings.set_gtk_application_prefer_dark_theme(true);
    }

    let cfg = load_config(DEFAULT_CONFIG_PATH);
    let state_path = if demo { DEMO_STATE_PATH } else { DEFAULT_STATE_PATH };
    let saved_state = load_state(state_path);
    let user = saved_state.user.clone().unwrap_or_else(|| cfg.user.clone());
    let sessions = read_sessions_filtered(&cfg.hide_sessions, &cfg.default_session);

    let win = gtk4::ApplicationWindow::new(app);
    win.set_title(Some("inerium-greeter"));

    let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    win.set_child(Some(&root));

    // Обнаружение и сортировка мониторов
    let display = gdk4::Display::default().expect("Wayland display must be available");
    let gdk_mons = display.monitors();
    let n_mons = gdk_mons.n_items();
    let mut monitors = Vec::new();
    for i in 0..n_mons {
        if let Some(m) = gdk_mons.item(i).and_downcast::<gdk4::Monitor>() {
            monitors.push(m);
        }
    }
    monitors.sort_by_key(|m| m.geometry().x());

    let multi = monitors.len() > 1;
    let session_names: Vec<String> = sessions.iter().map(|s| s.name.clone()).collect();

    let initial_session_name = saved_state
        .session
        .as_ref()
        .unwrap_or(&cfg.default_session);

    let initial_session_idx = session_names
        .iter()
        .position(|n| n == initial_session_name)
        .unwrap_or(0);

    let state = Rc::new(RefCell::new(GreeterState {
        demo,
        _cfg: cfg.clone(),
        _state: saved_state,
        state_path,
        user: user.clone(),
        sessions: sessions.clone(),
        busy: false,
        has_error: false,
        syncing_pw: false,
        syncing_sess: false,
        pws: Vec::new(),
        gos: Vec::new(),
        sesses: Vec::new(),
        statuses: Vec::new(),
        clocks: Vec::new(),
        dates: Vec::new(),
    }));

    let num_panels = if monitors.is_empty() { 1 } else { monitors.len() };

    for i in 0..num_panels {
        let overlay = gtk4::Overlay::new();
        if multi {
            if let Some(m) = monitors.get(i) {
                let geom = m.geometry();
                overlay.set_size_request(geom.width(), -1);
                overlay.set_hexpand(false);
            } else {
                overlay.set_hexpand(true);
            }
        } else {
            overlay.set_hexpand(true);
        }
        overlay.set_vexpand(true);

        // Фоновое изображение
        let bg = gtk4::Picture::for_filename(&cfg.background);
        bg.set_content_fit(gtk4::ContentFit::Cover);
        bg.set_can_shrink(true);
        overlay.set_child(Some(&bg));

        // Центральный блок
        let center = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        center.set_halign(gtk4::Align::Center);
        center.set_valign(gtk4::Align::Center);
        overlay.add_overlay(&center);

        let clock = gtk4::Label::new(None);
        clock.add_css_class("clock");

        let date = gtk4::Label::new(None);
        date.add_css_class("date");
        date.set_margin_bottom(28);

        center.append(&clock);
        center.append(&date);
        state.borrow_mut().clocks.push(clock);
        state.borrow_mut().dates.push(date);

        // Карточка авторизации
        let card = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        card.add_css_class("card");
        card.set_size_request(420, -1);
        center.append(&card);

        // Верх: Аватарка + Имя пользователя
        let top = gtk4::Box::new(gtk4::Orientation::Horizontal, 14);
        let avatar = gtk4::Label::new(Some("󰀉"));
        avatar.add_css_class("avatar");
        top.append(&avatar);

        let names_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        names_box.set_valign(gtk4::Align::Center);
        let user_lbl = gtk4::Label::new(Some(&user));
        user_lbl.add_css_class("user");
        user_lbl.set_xalign(0.0);
        let sub_lbl = gtk4::Label::new(Some("Авторизация"));
        sub_lbl.add_css_class("user-sub");
        sub_lbl.set_xalign(0.0);
        names_box.append(&user_lbl);
        names_box.append(&sub_lbl);
        top.append(&names_box);
        card.append(&top);

        // Поле пароля + кнопка входа
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let pw = gtk4::PasswordEntry::new();
        pw.set_show_peek_icon(true);
        pw.set_hexpand(true);
        pw.set_placeholder_text(Some("Пароль"));
        row.append(&pw);

        let go = gtk4::Button::with_label("󰁔");
        go.add_css_class("go");
        go.set_tooltip_text(Some("Войти (Enter)"));
        row.append(&go);
        card.append(&row);

        // Синхронизация ввода пароля между мониторами
        let state_for_pw = state.clone();
        let pw_clone = pw.clone();
        pw.connect_changed(move |_| {
            if state_for_pw.borrow().syncing_pw {
                return;
            }
            state_for_pw.borrow_mut().syncing_pw = true;
            let text = pw_clone.text().to_string();
            for other in &state_for_pw.borrow().pws {
                if other != &pw_clone && other.text().as_str() != text {
                    other.set_text(&text);
                }
            }
            state_for_pw.borrow_mut().syncing_pw = false;
        });

        // Нажатие Enter или кнопки "Войти"
        let state_for_login = state.clone();
        pw.connect_activate(move |_| {
            trigger_login(state_for_login.clone());
        });

        let state_for_go = state.clone();
        go.connect_clicked(move |_| {
            trigger_login(state_for_go.clone());
        });

        state.borrow_mut().pws.push(pw);
        state.borrow_mut().gos.push(go);

        // Сессия
        let sess_lbl = gtk4::Label::new(Some("СЕССИЯ"));
        sess_lbl.add_css_class("label");
        sess_lbl.set_xalign(0.0);
        card.append(&sess_lbl);

        let state_for_sess = state.clone();
        let sess_idx = i;
        let selector = SessionSelector::new(
            session_names.clone(),
            initial_session_idx,
            move |idx| {
                if state_for_sess.borrow().syncing_sess {
                    return;
                }
                state_for_sess.borrow_mut().syncing_sess = true;
                for (other_i, other_sel) in state_for_sess.borrow().sesses.iter().enumerate() {
                    if other_i != sess_idx {
                        other_sel.set_selected(idx);
                    }
                }
                state_for_sess.borrow_mut().syncing_sess = false;
            },
        );
        card.append(selector.widget());
        state.borrow_mut().sesses.push(selector);

        // Статус
        let status = gtk4::Label::new(None);
        status.add_css_class("status");
        status.set_xalign(0.0);
        status.set_wrap(true);
        status.set_ellipsize(pango::EllipsizeMode::End);
        status.set_visible(false);
        card.append(&status);
        state.borrow_mut().statuses.push(status);

        // Кнопки выключения / перезагрузки внизу
        let bottom = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        bottom.set_halign(gtk4::Align::Center);
        bottom.set_valign(gtk4::Align::End);
        bottom.set_margin_bottom(28);

        let btn_reboot = gtk4::Button::with_label("󰜉  Перезагрузка");
        btn_reboot.add_css_class("end");
        btn_reboot.connect_clicked(move |_| {
            system_action(&["systemctl", "reboot"], demo);
        });

        let btn_poweroff = gtk4::Button::with_label("󰐥  Выключение");
        btn_poweroff.add_css_class("end");
        btn_poweroff.connect_clicked(move |_| {
            system_action(&["systemctl", "poweroff"], demo);
        });

        bottom.append(&btn_reboot);
        bottom.append(&btn_poweroff);
        overlay.add_overlay(&bottom);

        root.append(&overlay);
    }

    // Клавиатурный контроллер для авто-фокуса и отслеживания Caps Lock
    let key_ctrl = gtk4::EventControllerKey::new();
    let state_for_keys = state.clone();
    key_ctrl.connect_key_pressed(move |_, _, _, _| {
        state_for_keys.borrow().focus_pw();
        let st = state_for_keys.clone();
        glib::idle_add_local_once(move || {
            st.borrow().update_caps();
        });
        glib::Propagation::Proceed
    });

    let state_for_release = state.clone();
    key_ctrl.connect_key_released(move |_, _, _, _| {
        state_for_release.borrow().update_caps();
    });
    win.add_controller(key_ctrl);

    // Первоначальное обновление часов и таймер 1с
    update_time(&state);
    let state_for_tick = state.clone();
    glib::timeout_add_seconds_local(1, move || {
        update_time(&state_for_tick);
        glib::ControlFlow::Continue
    });

    win.fullscreen();
    win.present();
    state.borrow().focus_pw();

    GLOBAL_STATE.with(|gs| *gs.borrow_mut() = Some(state.clone()));
    GLOBAL_APP.with(|ga| *ga.borrow_mut() = Some(app.clone()));

    // Первоначальная проверка Caps Lock
    let state_for_init_caps = state.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
        state_for_init_caps.borrow().update_caps();
    });
}

fn update_time(state: &Rc<RefCell<GreeterState>>) {
    let (time_str, date_str) = format_datetime();
    let st = state.borrow();
    for c in &st.clocks {
        c.set_label(&time_str);
    }
    for d in &st.dates {
        d.set_label(&date_str);
    }
}

fn trigger_login(state: Rc<RefCell<GreeterState>>) {
    if state.borrow().busy {
        return;
    }

    // Закрываем открытые селекторы
    for s in &state.borrow().sesses {
        s.close();
    }

    let password = state.borrow().get_password();
    if password.is_empty() {
        state.borrow().set_status("Введите пароль", StatusKind::Warn);
        return;
    }

    if state.borrow().sessions.is_empty() {
        state.borrow().set_status("Нет сессий в /usr/share/wayland-sessions", StatusKind::Error);
        return;
    }

    let idx = state.borrow().sesses.first().map(|s| s.get_selected()).unwrap_or(0);
    let session = state.borrow().sessions.get(idx).cloned()
        .unwrap_or_else(|| state.borrow().sessions[0].clone());

    state.borrow_mut().set_busy(true);
    state.borrow().set_status("Вход…", StatusKind::Normal);

    let demo = state.borrow().demo;
    let username = state.borrow().user.clone();
    let cmd = session.cmd.clone();
    let session_name = session.name.clone();
    let state_path = state.borrow().state_path;

    std::thread::spawn(move || {
        let auth_result = if demo {
            let mut client = greetd::DemoGreetd::new();
            greetd::authenticate(&mut client, &username, &password, &cmd)
        } else {
            match greetd::RealGreetd::connect() {
                Ok(mut client) => greetd::authenticate(&mut client, &username, &password, &cmd),
                Err(e) => Err(e),
            }
        };

        glib::MainContext::default().invoke(move || {
            on_auth_complete(auth_result, &username, &session_name, state_path);
        });
    });
}

fn on_auth_complete(
    res: Result<(), String>,
    username: &str,
    session_name: &str,
    state_path: &'static str,
) {
    GLOBAL_STATE.with(|gs| {
        let state_opt = gs.borrow();
        let Some(state) = state_opt.as_ref() else { return };

        match res {
            Ok(()) => {
                let _ = save_state(state_path, username, session_name);
                GLOBAL_APP.with(|app_cell| {
                    if let Some(app) = app_cell.borrow().as_ref() {
                        app.quit();
                    }
                });
            }
            Err(err_msg) => {
                state.borrow_mut().set_busy(false);
                state.borrow_mut().has_error = true;

                for pw in &state.borrow().pws {
                    pw.set_text("");
                    pw.add_css_class("error");
                }
                state.borrow().set_status(&format!("󰅙  {}", err_msg), StatusKind::Error);
                state.borrow().focus_pw();

                let state_for_clear = state.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(2500), move || {
                    state_for_clear.borrow_mut().has_error = false;
                    for pw in &state_for_clear.borrow().pws {
                        pw.remove_css_class("error");
                    }
                    for st in &state_for_clear.borrow().statuses {
                        st.remove_css_class("error");
                    }
                    state_for_clear.borrow().update_caps();
                });
            }
        }
    });
}

fn main() -> glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let demo = args.iter().any(|a| a == "--demo");

    let app = gtk4::Application::builder()
        .application_id("org.inerium.greeter")
        .build();

    app.connect_activate(move |a| {
        build_ui(a, demo);
    });

    app.run_with_args(&Vec::<String>::new())
}
