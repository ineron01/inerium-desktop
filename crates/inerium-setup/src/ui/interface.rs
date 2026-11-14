use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Align, Box, Button, ContentFit, DropDown, Entry, Label, Orientation, Picture, PolicyType,
    Scale, ScrolledWindow, StringList, Switch, glib,
};

use crate::i18n::{Lang, tr};
use crate::model::{
    HyprProfile, IdleConfig, InputOverrides, SetupPaths, detect_battery_charge_threshold,
    ensure_default_wallpapers, parse_active_wallpaper, parse_binds_lua, parse_hypridle_conf,
    parse_inputs_lua, read_profile, scan_wallpapers, set_battery_charge_threshold, write_profile,
};
use crate::system::{
    apply_hypridle, apply_wallpaper, reset_interface_overrides, save_inputs_and_reload,
};
use crate::ui::StatusCallback;

const KB_TOGGLE_OPTIONS: &[(&str, &str, &str)] = &[
    ("grp:alt_shift_toggle", "Alt + Shift (Default)", "Alt + Shift (По умолчанию)"),
    ("grp:caps_toggle", "Caps Lock", "Caps Lock"),
    ("grp:ctrl_shift_toggle", "Ctrl + Shift", "Ctrl + Shift"),
    ("grp:win_space_toggle", "Super + Space", "Super + Space"),
];

const LAUNCHER_BIND_OPTIONS: &[(&str, &str, &str)] = &[
    ("SUPER_L", "Super (single tap release)", "Super (одиночное нажатие)"),
    ("SUPER + R", "Super + R", "Super + R"),
    ("SUPER + D", "Super + D", "Super + D"),
    ("ALT + Space", "Alt + Space (Spotlight style)", "Alt + Space (как в macOS / Spotlight)"),
];

const IDLE_PRESETS: &[(&str, &str, u32, u32, u32)] = &[
    ("5 min (3m dim / 5m lock / 8m off)", "5 мин (3м экран / 5м блок / 8м выкл)", 180, 300, 480),
    ("10 min (5m dim / 10m lock / 15m off)", "10 мин (5м экран / 10м блок / 15м выкл)", 300, 600, 900),
    ("15 min (10m dim / 15m lock / 20m off)", "15 мин (10м экран / 15м блок / 20м выкл)", 600, 900, 1200),
    ("Never (disabled)", "Никогда (отключить автоблокировку)", 0, 0, 0),
];

pub fn build_interface_tab(paths: SetupPaths, status_cb: StatusCallback, lang: Lang) -> Box {
    ensure_default_wallpapers(&paths);
    let outer = Box::new(Orientation::Vertical, 10);

    let scroll = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .vscrollbar_policy(PolicyType::Automatic)
        .vexpand(true)
        .build();

    let content = Box::new(Orientation::Vertical, 12);
    content.set_margin_end(4);

    // 1. Hyprland Profile Card
    let prof_card = Box::new(Orientation::Horizontal, 12);
    prof_card.add_css_class("setup-card");

    let prof_info = Box::new(Orientation::Vertical, 2);
    prof_info.set_hexpand(true);
    let prof_title = Label::new(Some(tr(
        lang,
        "Default Hyprland profile",
        "Профиль Hyprland по умолчанию",
    )));
    prof_title.add_css_class("card-title");
    prof_title.set_xalign(0.0);
    prof_info.append(&prof_title);

    let prof_sub = Label::new(Some(tr(
        lang,
        "Stored in ~/.config/hypr/profile (gitignored)",
        "Хранится в ~/.config/hypr/profile (не отслеживается Git)",
    )));
    prof_sub.add_css_class("card-muted");
    prof_sub.set_xalign(0.0);
    prof_info.append(&prof_sub);
    prof_card.append(&prof_info);

    let current_prof = Rc::new(RefCell::new(read_profile(&paths.profile_path())));
    let prof_seg = Box::new(Orientation::Horizontal, 2);
    prof_seg.add_css_class("seg-group");
    prof_seg.set_valign(Align::Center);

    let btn_waybar = Button::with_label(tr(lang, "Waybar (main)", "Waybar (основной)"));
    let btn_backup = Button::with_label(tr(lang, "Backup (fallback)", "Backup (резервный)"));
    btn_waybar.add_css_class("seg-btn");
    btn_backup.add_css_class("seg-btn");
    if *current_prof.borrow() == HyprProfile::Waybar {
        btn_waybar.add_css_class("active");
    } else {
        btn_backup.add_css_class("active");
    }
    prof_seg.append(&btn_waybar);
    prof_seg.append(&btn_backup);
    prof_card.append(&prof_seg);
    content.append(&prof_card);

    for (btn, target_prof) in [
        (&btn_waybar, HyprProfile::Waybar),
        (&btn_backup, HyprProfile::Backup),
    ] {
        let btn_w = btn_waybar.clone();
        let btn_b = btn_backup.clone();
        let current_prof = current_prof.clone();
        let paths = paths.clone();
        let status_cb = status_cb.clone();
        btn.connect_clicked(move |_| {
            if let Err(e) = write_profile(&paths.profile_path(), target_prof) {
                status_cb.error(&e);
                return;
            }
            *current_prof.borrow_mut() = target_prof;
            if target_prof == HyprProfile::Waybar {
                btn_w.add_css_class("active");
                btn_b.remove_css_class("active");
            } else {
                btn_b.add_css_class("active");
                btn_w.remove_css_class("active");
            }
            status_cb.ok(&match lang {
                Lang::En => format!(
                    "Profile '{}' saved (applies on next login)",
                    target_prof.as_str()
                ),
                Lang::Ru => format!(
                    "Профиль '{}' сохранён (применится при следующем входе)",
                    target_prof.as_str()
                ),
            });
        });
    }

    // 2. Quick Wallpaper Picker Card
    let wall_card = Box::new(Orientation::Vertical, 8);
    wall_card.add_css_class("setup-card");

    let wall_head = Box::new(Orientation::Horizontal, 8);
    let wall_info = Box::new(Orientation::Vertical, 2);
    wall_info.set_hexpand(true);
    let wall_title = Label::new(Some(tr(
        lang,
        "Quick wallpaper picker",
        "Быстрый выбор обоев",
    )));
    wall_title.add_css_class("card-title");
    wall_title.set_xalign(0.0);
    wall_info.append(&wall_title);

    let wall_sub = Label::new(Some(tr(
        lang,
        "Sources: ~/Pictures/Wallpapers & ~/Обои (applied via set-wallpaper)",
        "Источники: ~/Pictures/Wallpapers и ~/Обои (применяется через set-wallpaper)",
    )));
    wall_sub.add_css_class("card-muted");
    wall_sub.set_xalign(0.0);
    wall_info.append(&wall_sub);
    wall_head.append(&wall_info);
    wall_card.append(&wall_head);

    let wall_scroll = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Automatic)
        .vscrollbar_policy(PolicyType::Never)
        .kinetic_scrolling(true)
        .build();
    wall_scroll.set_size_request(-1, 134);

    let wall_strip = Box::new(Orientation::Horizontal, 10);
    let wallpapers = scan_wallpapers(&paths.wallpaper_dirs());
    let active_wall = std::fs::read_to_string(paths.hyprpaper_conf_path())
        .ok()
        .and_then(|s| parse_active_wallpaper(&s));

    let wall_buttons: Rc<RefCell<Vec<(PathBuf, Button)>>> = Rc::new(RefCell::new(Vec::new()));

    if wallpapers.is_empty() {
        let empty = Label::new(Some(tr(
            lang,
            "No images found in ~/Pictures/Wallpapers or ~/Обои",
            "Изображения не найдены в ~/Pictures/Wallpapers или ~/Обои",
        )));
        empty.add_css_class("card-muted");
        empty.set_margin_top(30);
        wall_strip.append(&empty);
    } else {
        for wpath in wallpapers {
            let btn = Button::new();
            btn.add_css_class("wall-card");
            if active_wall.as_ref().is_some_and(|a| a == &wpath) {
                btn.add_css_class("active");
            }

            let inner = Box::new(Orientation::Vertical, 4);
            let pic = Picture::for_filename(&wpath);
            pic.set_content_fit(ContentFit::Cover);
            pic.set_can_shrink(true);
            pic.set_size_request(144, 82);
            pic.add_css_class("wall-thumb");
            inner.append(&pic);

            let fname = wpath
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("wallpaper");
            let short = truncate_label(fname, 18);
            let lbl = Label::new(Some(&short));
            lbl.add_css_class("wall-name");
            inner.append(&lbl);

            btn.set_child(Some(&inner));
            wall_buttons.borrow_mut().push((wpath.clone(), btn.clone()));
            wall_strip.append(&btn);
        }
    }

    for (wpath, btn) in wall_buttons.borrow().iter() {
        let wpath = wpath.clone();
        let wall_buttons = wall_buttons.clone();
        let status_cb = status_cb.clone();
        btn.connect_clicked(move |_| {
            for (p, b) in wall_buttons.borrow().iter() {
                if p == &wpath {
                    b.add_css_class("active");
                } else {
                    b.remove_css_class("active");
                }
            }
            let wpath_bg = wpath.clone();
            let status_cb = status_cb.clone();
            status_cb.warn(tr(lang, "Applying wallpaper...", "Применение обоев..."));
            glib::MainContext::default().spawn_local(async move {
                let fname = wpath_bg
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("wallpaper")
                    .to_string();
                let res = gio::spawn_blocking(move || apply_wallpaper(&wpath_bg))
                    .await
                    .unwrap_or_else(|e| Err(format!("Task panicked: {e:?}")));
                match res {
                    Ok(()) => status_cb.ok(&match lang {
                        Lang::En => format!("Wallpaper '{fname}' applied"),
                        Lang::Ru => format!("Обои '{fname}' применены"),
                    }),
                    Err(e) => status_cb.error(&e),
                }
            });
        });
    }

    wall_scroll.set_child(Some(&wall_strip));
    wall_card.append(&wall_scroll);
    content.append(&wall_card);

    // 3. Keyboard, Mouse, Touchpad & Power Overrides Card
    let ovr_card = Box::new(Orientation::Vertical, 10);
    ovr_card.add_css_class("setup-card");

    let ovr_title = Label::new(Some(tr(
        lang,
        "Input, displays & power (local overrides)",
        "Ввод, дисплеи и питание (локальные overrides)",
    )));
    ovr_title.add_css_class("card-title");
    ovr_title.set_xalign(0.0);
    ovr_card.append(&ovr_title);

    let ovr_sub = Label::new(Some(tr(
        lang,
        "Saved to inputs_local.lua, binds_local.lua & hypridle_local.conf (gitignored)",
        "Сохраняется в inputs_local.lua, binds_local.lua и hypridle_local.conf (не отслеживается Git)",
    )));
    ovr_sub.add_css_class("card-muted");
    ovr_sub.set_xalign(0.0);
    ovr_card.append(&ovr_sub);

    let saved_inputs = std::fs::read_to_string(paths.inputs_local_path())
        .map(|s| parse_inputs_lua(&s))
        .unwrap_or_default();
    let saved_binds_launcher = std::fs::read_to_string(paths.binds_local_path())
        .map(|s| parse_binds_lua(&s))
        .unwrap_or_default();
    let saved_idle = std::fs::read_to_string(paths.hypridle_local_conf_path())
        .map(|s| parse_hypridle_conf(&s))
        .unwrap_or_default();

    // --- Section: Keyboard ---
    let kb_section = Box::new(Orientation::Vertical, 6);
    kb_section.add_css_class("setup-subcard");

    let kb_sec_title = Label::new(Some(tr(lang, "Keyboard layout & shortcuts", "Раскладка клавиатуры и хоткеи")));
    kb_sec_title.add_css_class("card-meta");
    kb_sec_title.set_xalign(0.0);
    kb_section.append(&kb_sec_title);

    let kb_row = Box::new(Orientation::Horizontal, 10);
    let kb_layout_box = Box::new(Orientation::Vertical, 4);
    kb_layout_box.set_hexpand(true);
    let kb_lbl = Label::new(Some(tr(
        lang,
        "Layouts (kb_layout)",
        "Раскладки (kb_layout)",
    )));
    kb_lbl.add_css_class("card-muted");
    kb_lbl.set_xalign(0.0);
    kb_layout_box.append(&kb_lbl);

    let kb_entry_row = Box::new(Orientation::Horizontal, 6);
    let kb_entry = Entry::new();
    kb_entry.add_css_class("setup-entry");
    kb_entry.set_hexpand(true);
    kb_entry.set_text(&saved_inputs.kb_layout);
    kb_entry_row.append(&kb_entry);

    for preset in ["us,ru", "us", "us,ru,ua"] {
        let pbtn = Button::with_label(preset);
        pbtn.add_css_class("btn-secondary");
        let e_clone = kb_entry.clone();
        pbtn.connect_clicked(move |_| e_clone.set_text(preset));
        kb_entry_row.append(&pbtn);
    }
    kb_layout_box.append(&kb_entry_row);
    kb_row.append(&kb_layout_box);

    // Layout switch shortcut dropdown
    let opt_box = Box::new(Orientation::Vertical, 4);
    let opt_lbl = Label::new(Some(tr(
        lang,
        "Layout switch shortcut",
        "Переключение раскладки",
    )));
    opt_lbl.add_css_class("card-muted");
    opt_lbl.set_xalign(0.0);
    opt_box.append(&opt_lbl);

    let opt_labels: Vec<&str> = KB_TOGGLE_OPTIONS
        .iter()
        .map(|(_, en, ru)| tr(lang, en, ru))
        .collect();
    let opt_dd = DropDown::new(Some(StringList::new(&opt_labels)), gtk4::Expression::NONE);
    opt_dd.add_css_class("setup-dropdown");
    let opt_idx = KB_TOGGLE_OPTIONS
        .iter()
        .position(|(val, _, _)| *val == saved_inputs.kb_options)
        .unwrap_or(0);
    opt_dd.set_selected(opt_idx as u32);
    opt_box.append(&opt_dd);
    kb_row.append(&opt_box);
    kb_section.append(&kb_row);

    // Dynamic conflict warning & launcher shortcut selector
    let conflict_box = Box::new(Orientation::Vertical, 4);
    conflict_box.add_css_class("setup-subcard");
    conflict_box.set_margin_top(4);

    let conflict_warn_lbl = Label::new(Some(tr(
        lang,
        "⚠ Super + Space is bound to layout toggle. Choose a new shortcut for Application Launcher:",
        "⚠ Super + Space занят сменой языка. Выберите новую клавишу для Лаунчера приложений:",
    )));
    conflict_warn_lbl.add_css_class("card-title");
    conflict_warn_lbl.set_xalign(0.0);
    conflict_warn_lbl.set_wrap(true);
    conflict_box.append(&conflict_warn_lbl);

    let launcher_row = Box::new(Orientation::Horizontal, 8);
    let launcher_lbl = Label::new(Some(tr(lang, "Launcher Shortcut:", "Клавиша лаунчера:")));
    launcher_lbl.add_css_class("card-meta");
    launcher_row.append(&launcher_lbl);

    let launcher_labels: Vec<&str> = LAUNCHER_BIND_OPTIONS
        .iter()
        .map(|(_, en, ru)| tr(lang, en, ru))
        .collect();
    let launcher_dd = DropDown::new(Some(StringList::new(&launcher_labels)), gtk4::Expression::NONE);
    launcher_dd.add_css_class("setup-dropdown");
    launcher_dd.set_hexpand(true);

    let cur_launcher = saved_binds_launcher.as_deref().unwrap_or("SUPER + R");
    let launcher_idx = LAUNCHER_BIND_OPTIONS
        .iter()
        .position(|(val, _, _)| *val == cur_launcher)
        .unwrap_or(1);
    launcher_dd.set_selected(launcher_idx as u32);
    launcher_row.append(&launcher_dd);
    conflict_box.append(&launcher_row);

    conflict_box.set_visible(saved_inputs.kb_options == "grp:win_space_toggle");

    {
        let conflict_box_c = conflict_box.clone();
        opt_dd.connect_selected_notify(move |dd| {
            let idx = dd.selected() as usize;
            let opt = KB_TOGGLE_OPTIONS.get(idx).map(|(v, _, _)| *v).unwrap_or("");
            conflict_box_c.set_visible(opt == "grp:win_space_toggle");
        });
    }

    kb_section.append(&conflict_box);
    ovr_card.append(&kb_section);

    // --- Section: Mouse ---
    let mouse_section = Box::new(Orientation::Vertical, 6);
    mouse_section.add_css_class("setup-subcard");

    let mouse_sec_title = Label::new(Some(tr(lang, "Mouse settings", "Настройки мыши")));
    mouse_sec_title.add_css_class("card-meta");
    mouse_sec_title.set_xalign(0.0);
    mouse_section.append(&mouse_sec_title);

    // Sensitivity slider [-1.0 .. 1.0]
    let sens_row = Box::new(Orientation::Horizontal, 10);
    let sens_lbl = Label::new(Some(tr(
        lang,
        "Sensitivity [-1.0 .. 1.0]:",
        "Чувствительность [-1.0 .. 1.0]:",
    )));
    sens_lbl.add_css_class("card-meta");
    sens_row.append(&sens_lbl);

    let sens_scale = Scale::with_range(Orientation::Horizontal, -1.0, 1.0, 0.05);
    sens_scale.set_hexpand(true);
    sens_scale.set_value(saved_inputs.sensitivity);
    sens_row.append(&sens_scale);

    let sens_val_lbl = Label::new(Some(&format!("{:.2}", saved_inputs.sensitivity)));
    sens_val_lbl.add_css_class("card-meta");
    sens_val_lbl.set_width_chars(5);
    sens_row.append(&sens_val_lbl);

    let sens_reset_btn = Button::with_label("⟲ 0.0");
    sens_reset_btn.add_css_class("btn-secondary");
    sens_reset_btn.set_tooltip_text(Some(tr(lang, "Reset to default (0.0)", "Сбросить на 0.0")));
    {
        let sens_scale_c = sens_scale.clone();
        sens_reset_btn.connect_clicked(move |_| {
            sens_scale_c.set_value(0.0);
        });
    }
    sens_row.append(&sens_reset_btn);

    {
        let sens_val_lbl_c = sens_val_lbl.clone();
        sens_scale.connect_value_changed(move |sc| {
            sens_val_lbl_c.set_text(&format!("{:.2}", sc.value()));
        });
    }
    mouse_section.append(&sens_row);

    // Mouse Accel profile switch
    let sw_flat = make_switch_row(
        &mouse_section,
        tr(
            lang,
            "Flat profile (No acceleration — recommended for gaming)",
            "Без ускорения (Flat profile — идеально для игр)",
        ),
        saved_inputs.accel_profile == "flat",
    );

    // Scroll speed slider [0.2 .. 3.0]
    let scroll_row = Box::new(Orientation::Horizontal, 10);
    let scroll_lbl = Label::new(Some(tr(
        lang,
        "Scroll speed [0.2 .. 3.0]:",
        "Скорость прокрутки [0.2 .. 3.0]:",
    )));
    scroll_lbl.add_css_class("card-meta");
    scroll_row.append(&scroll_lbl);

    let scroll_scale = Scale::with_range(Orientation::Horizontal, 0.2, 3.0, 0.1);
    scroll_scale.set_hexpand(true);
    scroll_scale.set_value(saved_inputs.scroll_factor);
    scroll_row.append(&scroll_scale);

    let scroll_val_lbl = Label::new(Some(&format!("{:.1}", saved_inputs.scroll_factor)));
    scroll_val_lbl.add_css_class("card-meta");
    scroll_val_lbl.set_width_chars(5);
    scroll_row.append(&scroll_val_lbl);

    let scroll_reset_btn = Button::with_label("⟲ 1.0");
    scroll_reset_btn.add_css_class("btn-secondary");
    scroll_reset_btn.set_tooltip_text(Some(tr(lang, "Reset to default (1.0)", "Сбросить на 1.0")));
    {
        let scroll_scale_c = scroll_scale.clone();
        scroll_reset_btn.connect_clicked(move |_| {
            scroll_scale_c.set_value(1.0);
        });
    }
    scroll_row.append(&scroll_reset_btn);

    {
        let scroll_val_lbl_c = scroll_val_lbl.clone();
        scroll_scale.connect_value_changed(move |sc| {
            scroll_val_lbl_c.set_text(&format!("{:.1}", sc.value()));
        });
    }
    mouse_section.append(&scroll_row);

    ovr_card.append(&mouse_section);

    // --- Section: Touchpad ---
    let tp_section = Box::new(Orientation::Vertical, 6);
    tp_section.add_css_class("setup-subcard");

    let tp_sec_title = Label::new(Some(tr(lang, "Touchpad settings", "Настройки тачпада")));
    tp_sec_title.add_css_class("card-meta");
    tp_sec_title.set_xalign(0.0);
    tp_section.append(&tp_sec_title);

    let sw_natural = make_switch_row(
        &tp_section,
        tr(
            lang,
            "Natural scrolling (content follows fingers, smartphone-like)",
            "Естественная прокрутка (инверсия как на смартфоне)",
        ),
        saved_inputs.natural_scroll,
    );
    let sw_tap = make_switch_row(
        &tp_section,
        tr(lang, "Tap to click", "Касание для клика (Tap-to-click)"),
        saved_inputs.tap_to_click,
    );
    let sw_dwt = make_switch_row(
        &tp_section,
        tr(lang, "Disable while typing", "Отключать при наборе текста (Disable while typing)"),
        saved_inputs.disable_while_typing,
    );
    ovr_card.append(&tp_section);

    // --- Section: Screen & Power ---
    let pwr_section = Box::new(Orientation::Vertical, 6);
    pwr_section.add_css_class("setup-subcard");

    let pwr_sec_title = Label::new(Some(tr(lang, "Screen & Power", "Экран и электропитание")));
    pwr_sec_title.add_css_class("card-meta");
    pwr_sec_title.set_xalign(0.0);
    pwr_section.append(&pwr_sec_title);

    // Autolock row
    let idle_row = Box::new(Orientation::Horizontal, 12);
    let idle_lbl = Label::new(Some(tr(
        lang,
        "Screen lock timeout (hypridle):",
        "Тайм-аут автоблокировки экрана:",
    )));
    idle_lbl.add_css_class("card-meta");
    idle_row.append(&idle_lbl);

    let idle_labels: Vec<&str> = IDLE_PRESETS
        .iter()
        .map(|(en, ru, _, _, _)| tr(lang, en, ru))
        .collect();
    let idle_dd = DropDown::new(Some(StringList::new(&idle_labels)), gtk4::Expression::NONE);
    idle_dd.add_css_class("setup-dropdown");
    idle_dd.set_hexpand(true);

    let idle_idx = if !saved_idle.enabled {
        3 // Never
    } else {
        IDLE_PRESETS
            .iter()
            .position(|&(_, _, d, l, dp)| {
                d == saved_idle.dim_secs && l == saved_idle.lock_secs && dp == saved_idle.dpms_secs
            })
            .unwrap_or(1) // Default: 10 min
    };
    idle_dd.set_selected(idle_idx as u32);
    idle_row.append(&idle_dd);
    pwr_section.append(&idle_row);

    // Battery threshold row (if supported)
    let bat_supported = detect_battery_charge_threshold();
    let sw_bat_limit: Option<Switch> = if let Some((_, current_th)) = bat_supported {
        let sw = make_switch_row(
            &pwr_section,
            tr(
                lang,
                "Battery charge limit 80% (Preserves battery lifespan on AC)",
                "Лимит заряда батареи 80% (Продлевает срок службы аккумулятора)",
            ),
            current_th <= 85,
        );
        Some(sw)
    } else {
        None
    };

    ovr_card.append(&pwr_section);

    // Action buttons for overrides
    let ovr_actions = Box::new(Orientation::Horizontal, 8);
    let ovr_reset_btn = Button::with_label(tr(lang, "Reset overrides", "Сбросить overrides"));
    ovr_reset_btn.add_css_class("btn-danger");
    ovr_actions.append(&ovr_reset_btn);

    let spacer = Box::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    ovr_actions.append(&spacer);

    let ovr_save_btn = Button::with_label(tr(lang, "Save overrides", "Сохранить overrides"));
    ovr_save_btn.add_css_class("btn-primary");
    ovr_actions.append(&ovr_save_btn);
    ovr_card.append(&ovr_actions);

    content.append(&ovr_card);

    // Wire Save Overrides
    {
        let paths = paths.clone();
        let status_cb = status_cb.clone();
        let kb_entry = kb_entry.clone();
        let opt_dd = opt_dd.clone();
        let launcher_dd = launcher_dd.clone();
        let sens_scale = sens_scale.clone();
        let sw_flat = sw_flat.clone();
        let scroll_scale = scroll_scale.clone();
        let sw_natural = sw_natural.clone();
        let sw_tap = sw_tap.clone();
        let sw_dwt = sw_dwt.clone();
        let idle_dd = idle_dd.clone();
        let sw_bat_limit = sw_bat_limit.clone();

        ovr_save_btn.connect_clicked(move |_| {
            let layout = kb_entry.text().trim().to_string();
            if layout.is_empty() {
                status_cb.error(tr(
                    lang,
                    "kb_layout cannot be empty",
                    "Поле kb_layout не может быть пустым",
                ));
                return;
            }
            let opt_idx = opt_dd.selected() as usize;
            let kb_options = KB_TOGGLE_OPTIONS
                .get(opt_idx)
                .map(|(v, _, _)| *v)
                .unwrap_or("grp:alt_shift_toggle")
                .to_string();

            let launcher_bind = if kb_options == "grp:win_space_toggle" {
                let l_idx = launcher_dd.selected() as usize;
                LAUNCHER_BIND_OPTIONS
                    .get(l_idx)
                    .map(|(v, _, _)| v.to_string())
            } else {
                None
            };

            let input_cfg = InputOverrides {
                kb_layout: layout,
                kb_options,
                natural_scroll: sw_natural.is_active(),
                tap_to_click: sw_tap.is_active(),
                disable_while_typing: sw_dwt.is_active(),
                sensitivity: sens_scale.value(),
                accel_profile: if sw_flat.is_active() { "flat".to_string() } else { "adaptive".to_string() },
                scroll_factor: scroll_scale.value(),
                launcher_bind,
            };

            let idle_idx = idle_dd.selected() as usize;
            let &(_, _, dim_secs, lock_secs, dpms_secs) =
                IDLE_PRESETS.get(idle_idx).unwrap_or(&IDLE_PRESETS[1]);
            let idle_cfg = IdleConfig {
                enabled: idle_idx != 3, // 3 is "Never"
                dim_secs,
                lock_secs,
                dpms_secs,
            };

            if let Some(ref sw_bat) = sw_bat_limit {
                let target_th = if sw_bat.is_active() { 80 } else { 100 };
                let _ = set_battery_charge_threshold(target_th);
            }

            if let Err(e) = save_inputs_and_reload(&paths, &input_cfg) {
                status_cb.error(&e);
                return;
            }
            if let Err(e) = apply_hypridle(&paths, &idle_cfg) {
                status_cb.warn(&match lang {
                    Lang::En => format!("Input applied, but hypridle: {e}"),
                    Lang::Ru => format!("Ввод применён, но hypridle: {e}"),
                });
                return;
            }
            status_cb.ok(tr(
                lang,
                "Keyboard, mouse, touchpad & hypridle overrides applied",
                "Настройки клавиатуры, мыши, тачпада и hypridle применены",
            ));
        });
    }

    // Wire Reset Overrides
    {
        let paths = paths.clone();
        let status_cb = status_cb.clone();
        let kb_entry = kb_entry.clone();
        let opt_dd = opt_dd.clone();
        let sens_scale = sens_scale.clone();
        let sw_flat = sw_flat.clone();
        let scroll_scale = scroll_scale.clone();
        let sw_natural = sw_natural.clone();
        let sw_tap = sw_tap.clone();
        let sw_dwt = sw_dwt.clone();
        let idle_dd = idle_dd.clone();
        let sw_bat_limit = sw_bat_limit.clone();

        ovr_reset_btn.connect_clicked(move |_| match reset_interface_overrides(&paths) {
            Ok(()) => {
                let def_in = InputOverrides::default();
                kb_entry.set_text(&def_in.kb_layout);
                opt_dd.set_selected(0);
                sens_scale.set_value(def_in.sensitivity);
                sw_flat.set_active(def_in.accel_profile == "flat");
                scroll_scale.set_value(def_in.scroll_factor);
                sw_natural.set_active(def_in.natural_scroll);
                sw_tap.set_active(def_in.tap_to_click);
                sw_dwt.set_active(def_in.disable_while_typing);
                idle_dd.set_selected(1);
                if let Some(ref sw_bat) = sw_bat_limit {
                    sw_bat.set_active(true);
                    let _ = set_battery_charge_threshold(80);
                }
                status_cb.ok(tr(
                    lang,
                    "Local overrides reset to base config",
                    "Локальные overrides сброшены на базовый конфиг",
                ));
            }
            Err(e) => status_cb.error(&e),
        });
    }

    scroll.set_child(Some(&content));
    outer.append(&scroll);
    outer
}

fn make_switch_row(parent: &Box, label: &str, active: bool) -> Switch {
    let row = Box::new(Orientation::Horizontal, 8);
    row.set_hexpand(true);
    let lbl = Label::new(Some(label));
    lbl.add_css_class("card-meta");
    lbl.set_xalign(0.0);
    lbl.set_hexpand(true);
    row.append(&lbl);

    let sw = Switch::new();
    sw.add_css_class("setup-switch");
    sw.set_active(active);
    sw.set_valign(Align::Center);
    row.append(&sw);
    parent.append(&row);
    sw
}

fn truncate_label(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let collected: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{collected}…")
    } else {
        collected
    }
}
