pub mod displays;
pub mod integrations;
pub mod interface;

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Align, Application, ApplicationWindow, Box, Button, Label, Orientation, Stack,
    StackTransitionType,
};

use crate::model::{SetupPaths, StepStatus, mark_setup_done};
use crate::system::evaluate_steps;

#[derive(Clone)]
pub struct StatusCallback {
    label: Label,
}

impl StatusCallback {
    pub fn new(label: Label) -> Self {
        Self { label }
    }

    pub fn ok(&self, msg: &str) {
        self.label.remove_css_class("err");
        self.label.add_css_class("ok");
        self.label.set_text(&format!("✓ {msg}"));
    }

    pub fn warn(&self, msg: &str) {
        self.label.remove_css_class("ok");
        self.label.remove_css_class("err");
        self.label.set_text(&format!("• {msg}"));
    }

    pub fn error(&self, msg: &str) {
        self.label.remove_css_class("ok");
        self.label.add_css_class("err");
        self.label.set_text(&format!("✕ {msg}"));
    }
}

pub fn build_main_window(
    app: &Application,
    paths: SetupPaths,
    autostart_mode: bool,
    start_on_integrations: bool,
) -> ApplicationWindow {
    let win = ApplicationWindow::builder()
        .application(app)
        .title("Настройка Kanagawa")
        .default_width(900)
        .default_height(640)
        .resizable(true)
        .build();
    win.add_css_class("setup-window");

    let shell = Box::new(Orientation::Vertical, 12);
    shell.add_css_class("setup-shell");

    // Header
    let header = Box::new(Orientation::Horizontal, 12);
    header.add_css_class("setup-header");

    let brand_ico = Label::new(Some("󰒓"));
    brand_ico.add_css_class("setup-brand-ico");
    brand_ico.set_halign(Align::Center);
    brand_ico.set_valign(Align::Center);
    header.append(&brand_ico);

    let title_box = Box::new(Orientation::Vertical, 2);
    title_box.set_hexpand(true);
    let title = Label::new(Some("Настройка Kanagawa"));
    title.add_css_class("setup-title");
    title.set_xalign(0.0);
    title_box.append(&title);

    let subtitle_text = if autostart_mode {
        "Первичная настройка рабочего окружения Hyprland"
    } else {
        "Мониторы, рабочие столы, профиль, обои и интеграции"
    };
    let subtitle = Label::new(Some(subtitle_text));
    subtitle.add_css_class("setup-subtitle");
    subtitle.set_xalign(0.0);
    title_box.append(&subtitle);
    header.append(&title_box);

    // Navigation Tabs
    let tabs_bar = Box::new(Orientation::Horizontal, 4);
    tabs_bar.add_css_class("setup-tabs");
    tabs_bar.set_valign(Align::Center);

    let tab_displays_btn = Button::with_label("󰍹 Рабочие столы и мониторы");
    let tab_interface_btn = Button::with_label("󰸉 Интерфейс и профиль");
    let tab_int_btn = Button::new();

    for b in [&tab_displays_btn, &tab_interface_btn, &tab_int_btn] {
        b.add_css_class("setup-tab-btn");
    }

    let initial_steps = evaluate_steps(&paths);
    let initial_pending = initial_steps
        .iter()
        .filter(|s| s.status == StepStatus::Pending)
        .count();
    let has_steps = !initial_steps.is_empty();

    let int_btn_inner = Box::new(Orientation::Horizontal, 6);
    let int_btn_lbl = Label::new(Some("󰌆 Аккаунты и интеграции"));
    let int_badge = Label::new(Some(&initial_pending.to_string()));
    int_badge.add_css_class("setup-badge");
    int_badge.set_visible(initial_pending > 0);
    int_btn_inner.append(&int_btn_lbl);
    int_btn_inner.append(&int_badge);
    tab_int_btn.set_child(Some(&int_btn_inner));
    tab_int_btn.set_visible(has_steps);

    tabs_bar.append(&tab_displays_btn);
    tabs_bar.append(&tab_interface_btn);
    tabs_bar.append(&tab_int_btn);
    header.append(&tabs_bar);
    shell.append(&header);

    // Footer status + action buttons (created early so tabs get StatusCallback)
    let footer = Box::new(Orientation::Horizontal, 10);
    footer.add_css_class("setup-footer");

    let status_lbl = Label::new(Some("Готово к настройке"));
    status_lbl.add_css_class("status-msg");
    status_lbl.set_xalign(0.0);
    status_lbl.set_hexpand(true);
    status_lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    footer.append(&status_lbl);

    let status_cb = StatusCallback::new(status_lbl);

    // Content Stack
    let stack = Stack::builder()
        .transition_type(StackTransitionType::Crossfade)
        .transition_duration(140)
        .vexpand(true)
        .hexpand(true)
        .build();

    let displays_page = displays::build_displays_tab(paths.clone(), status_cb.clone());
    let interface_page = interface::build_interface_tab(paths.clone(), status_cb.clone());
    let integrations_page = integrations::build_integrations_tab(
        paths.clone(),
        initial_steps,
        status_cb.clone(),
        {
            let int_badge = int_badge.clone();
            move |pending| {
                int_badge.set_text(&pending.to_string());
                int_badge.set_visible(pending > 0);
            }
        },
    );

    stack.add_named(&displays_page, Some("displays"));
    stack.add_named(&interface_page, Some("interface"));
    stack.add_named(&integrations_page, Some("integrations"));
    shell.append(&stack);

    let tab_buttons: Rc<RefCell<Vec<(&'static str, Button)>>> = Rc::new(RefCell::new(vec![
        ("displays", tab_displays_btn.clone()),
        ("interface", tab_interface_btn.clone()),
        ("integrations", tab_int_btn.clone()),
    ]));

    let select_tab = {
        let stack = stack.clone();
        let tab_buttons = tab_buttons.clone();
        Rc::new(move |name: &str| {
            stack.set_visible_child_name(name);
            for (n, b) in tab_buttons.borrow().iter() {
                if *n == name {
                    b.add_css_class("active");
                } else {
                    b.remove_css_class("active");
                }
            }
        })
    };

    for (name, btn) in tab_buttons.borrow().iter() {
        let select_tab = select_tab.clone();
        let name = *name;
        btn.connect_clicked(move |_| select_tab(name));
    }

    if start_on_integrations && has_steps {
        select_tab("integrations");
    } else {
        select_tab("displays");
    }

    // Footer buttons
    if autostart_mode {
        let later_btn = Button::with_label("Настроить позже");
        later_btn.add_css_class("btn-secondary");
        let win_c = win.clone();
        later_btn.connect_clicked(move |_| win_c.close());
        footer.append(&later_btn);

        let finish_btn = Button::with_label("Завершить настройку");
        finish_btn.add_css_class("btn-primary");
        let win_c = win.clone();
        let paths_c = paths.clone();
        finish_btn.connect_clicked(move |_| {
            let _ = mark_setup_done(&paths_c);
            win_c.close();
        });
        footer.append(&finish_btn);
    } else {
        let close_btn = Button::with_label("Закрыть");
        close_btn.add_css_class("btn-secondary");
        let win_c = win.clone();
        let paths_c = paths.clone();
        close_btn.connect_clicked(move |_| {
            let _ = mark_setup_done(&paths_c);
            win_c.close();
        });
        footer.append(&close_btn);
    }

    shell.append(&footer);
    win.set_child(Some(&shell));
    win
}
