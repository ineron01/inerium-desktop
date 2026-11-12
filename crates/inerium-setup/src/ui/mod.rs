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

use crate::i18n::{Lang, tr};
use crate::model::{SetupPaths, StepStatus, mark_setup_done};
use crate::system::{evaluate_steps, self_remove_onboarding};

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
    let lang = Lang::default();
    let win = ApplicationWindow::builder()
        .application(app)
        .title(tr(lang, "Inerium Setup", "Настройка Inerium"))
        .default_width(900)
        .default_height(640)
        .resizable(true)
        .build();
    win.add_css_class("setup-window");

    let initial_tab = Rc::new(RefCell::new(if start_on_integrations {
        "integrations".to_string()
    } else {
        "displays".to_string()
    }));

    let rebuild: RebuildCb = Rc::new(RefCell::new(None));
    let rebuild_fn: Rc<dyn Fn(Lang)> = {
        let win = win.clone();
        let paths = paths.clone();
        let active_tab = initial_tab.clone();
        let rebuild_ref = rebuild.clone();
        Rc::new(move |new_lang: Lang| {
            win.set_title(Some(tr(new_lang, "Inerium Setup", "Настройка Inerium")));
            let shell = build_shell(
                &win,
                &paths,
                autostart_mode,
                active_tab.clone(),
                new_lang,
                rebuild_ref.clone(),
            );
            win.set_child(Some(&shell));
        })
    };
    *rebuild.borrow_mut() = Some(rebuild_fn.clone());
    rebuild_fn(lang);

    win
}

type RebuildCb = Rc<RefCell<Option<Rc<dyn Fn(Lang)>>>>;

fn build_shell(
    win: &ApplicationWindow,
    paths: &SetupPaths,
    autostart_mode: bool,
    active_tab: Rc<RefCell<String>>,
    lang: Lang,
    rebuild_ref: RebuildCb,
) -> Box {
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
    let title = Label::new(Some(tr(lang, "Inerium Setup", "Настройка Inerium")));
    title.add_css_class("setup-title");
    title.set_xalign(0.0);
    title_box.append(&title);

    let subtitle_text = if autostart_mode {
        tr(
            lang,
            "First-time Hyprland desktop setup",
            "Первичная настройка рабочего окружения Hyprland",
        )
    } else {
        tr(
            lang,
            "Displays, workspaces, interface, wallpapers & integrations",
            "Мониторы, рабочие столы, профиль, обои и интеграции",
        )
    };
    let subtitle = Label::new(Some(subtitle_text));
    subtitle.add_css_class("setup-subtitle");
    subtitle.set_xalign(0.0);
    title_box.append(&subtitle);
    header.append(&title_box);

    // Language toggle (EN / RU)
    let lang_seg = Box::new(Orientation::Horizontal, 2);
    lang_seg.add_css_class("seg-group");
    lang_seg.set_valign(Align::Center);
    let btn_en = Button::with_label("EN");
    let btn_ru = Button::with_label("RU");
    btn_en.add_css_class("seg-btn");
    btn_ru.add_css_class("seg-btn");
    match lang {
        Lang::En => btn_en.add_css_class("active"),
        Lang::Ru => btn_ru.add_css_class("active"),
    }
    lang_seg.append(&btn_en);
    lang_seg.append(&btn_ru);
    header.append(&lang_seg);

    for (btn, target_lang) in [(btn_en, Lang::En), (btn_ru, Lang::Ru)] {
        let rebuild_ref = rebuild_ref.clone();
        btn.connect_clicked(move |_| {
            if target_lang != lang
                && let Some(ref f) = *rebuild_ref.borrow()
            {
                f(target_lang);
            }
        });
    }

    // Navigation Tabs
    let tabs_bar = Box::new(Orientation::Horizontal, 4);
    tabs_bar.add_css_class("setup-tabs");
    tabs_bar.set_valign(Align::Center);

    let tab_displays_btn = Button::with_label(tr(
        lang,
        "󰍹 Displays & Workspaces",
        "󰍹 Рабочие столы и мониторы",
    ));
    let tab_interface_btn = Button::with_label(tr(
        lang,
        "󰸉 Interface & Profile",
        "󰸉 Интерфейс и профиль",
    ));
    let tab_int_btn = Button::new();

    for b in [&tab_displays_btn, &tab_interface_btn, &tab_int_btn] {
        b.add_css_class("setup-tab-btn");
    }

    let initial_steps = evaluate_steps(paths);
    let initial_pending = initial_steps
        .iter()
        .filter(|s| s.status == StepStatus::Pending)
        .count();
    let has_steps = !initial_steps.is_empty();

    let int_btn_inner = Box::new(Orientation::Horizontal, 6);
    let int_btn_lbl = Label::new(Some(tr(
        lang,
        "󰌆 Accounts & Integrations",
        "󰌆 Аккаунты и интеграции",
    )));
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

    // Footer status + action buttons
    let footer = Box::new(Orientation::Horizontal, 10);
    footer.add_css_class("setup-footer");

    let status_lbl = Label::new(Some(tr(lang, "Ready", "Готово к настройке")));
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

    let displays_page = displays::build_displays_tab(paths.clone(), status_cb.clone(), lang);
    let interface_page = interface::build_interface_tab(paths.clone(), status_cb.clone(), lang);
    let integrations_page = integrations::build_integrations_tab(
        paths.clone(),
        initial_steps,
        status_cb.clone(),
        lang,
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
        let active_tab = active_tab.clone();
        Rc::new(move |name: &str| {
            *active_tab.borrow_mut() = name.to_string();
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

    let current_tab = active_tab.borrow().clone();
    if current_tab == "integrations" && has_steps {
        select_tab("integrations");
    } else if current_tab == "interface" {
        select_tab("interface");
    } else {
        select_tab("displays");
    }

    // Footer buttons
    if autostart_mode {
        let later_btn = Button::with_label(tr(lang, "Setup Later", "Настроить позже"));
        later_btn.add_css_class("btn-secondary");
        let win_c = win.clone();
        later_btn.connect_clicked(move |_| {
            win_c.close();
        });
        footer.append(&later_btn);

        let finish_btn = Button::with_label(tr(lang, "Finish Setup", "Завершить настройку"));
        finish_btn.add_css_class("btn-primary");
        let win_c = win.clone();
        let paths_c = paths.clone();
        finish_btn.connect_clicked(move |_| {
            let _ = mark_setup_done(&paths_c);
            let _ = self_remove_onboarding(&paths_c);
            win_c.close();
        });
        footer.append(&finish_btn);
    } else {
        let close_btn = Button::with_label(tr(lang, "Close", "Закрыть"));
        close_btn.add_css_class("btn-secondary");
        let win_c = win.clone();
        close_btn.connect_clicked(move |_| {
            win_c.close();
        });
        footer.append(&close_btn);
    }

    shell.append(&footer);
    shell
}
