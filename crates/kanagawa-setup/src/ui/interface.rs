use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Align, Box, Button, ContentFit, DropDown, Entry, Label, Orientation, Picture, PolicyType,
    ScrolledWindow, StringList, Switch, glib,
};

use crate::model::{
    HyprProfile, IdleConfig, InputOverrides, SetupPaths, parse_active_wallpaper,
    parse_hypridle_conf, parse_inputs_lua, read_profile, scan_wallpapers, write_profile,
};
use crate::system::{
    apply_hypridle, apply_wallpaper, reset_interface_overrides, save_inputs_and_reload,
};
use crate::ui::StatusCallback;

const KB_TOGGLE_OPTIONS: &[(&str, &str)] = &[
    ("grp:alt_shift_toggle", "Alt + Shift"),
    ("grp:win_space_toggle", "Super + Space"),
    ("grp:caps_toggle", "Caps Lock"),
    ("grp:ctrl_shift_toggle", "Ctrl + Shift"),
];

const IDLE_PRESETS: &[(&str, u32, u32, u32)] = &[
    ("Быстро (2 / 4 / 6 мин)", 120, 240, 360),
    ("Стандарт (4 / 7 / 10 мин)", 240, 420, 600),
    ("Долго (10 / 15 / 20 мин)", 600, 900, 1200),
    ("Презентация (15 / 30 / 45 мин)", 900, 1800, 2700),
];

pub fn build_interface_tab(paths: SetupPaths, status_cb: StatusCallback) -> Box {
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
    let prof_title = Label::new(Some("Профиль Hyprland по умолчанию"));
    prof_title.add_css_class("card-title");
    prof_title.set_xalign(0.0);
    prof_info.append(&prof_title);

    let prof_sub = Label::new(Some(
        "Хранится в ~/.config/hypr/profile (не отслеживается Git)",
    ));
    prof_sub.add_css_class("card-muted");
    prof_sub.set_xalign(0.0);
    prof_info.append(&prof_sub);
    prof_card.append(&prof_info);

    let current_prof = Rc::new(RefCell::new(read_profile(&paths.profile_path())));
    let prof_seg = Box::new(Orientation::Horizontal, 2);
    prof_seg.add_css_class("seg-group");
    prof_seg.set_valign(Align::Center);

    let btn_waybar = Button::with_label("Waybar (основной)");
    let btn_backup = Button::with_label("Backup (резервный)");
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
            status_cb.ok(&format!(
                "Профиль '{}' сохранён (применится при следующем входе)",
                target_prof.as_str()
            ));
        });
    }

    // 2. Quick Wallpaper Picker Card
    let wall_card = Box::new(Orientation::Vertical, 8);
    wall_card.add_css_class("setup-card");

    let wall_head = Box::new(Orientation::Horizontal, 8);
    let wall_info = Box::new(Orientation::Vertical, 2);
    wall_info.set_hexpand(true);
    let wall_title = Label::new(Some("Быстрый выбор обоев"));
    wall_title.add_css_class("card-title");
    wall_title.set_xalign(0.0);
    wall_info.append(&wall_title);

    let wall_sub = Label::new(Some(
        "Источники: ~/Pictures/Wallpapers и ~/Обои (применяется через set-wallpaper)",
    ));
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
        let empty = Label::new(Some("Изображения не найдены в ~/Pictures/Wallpapers или ~/Обои"));
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
            status_cb.warn("Применение обоев...");
            glib::MainContext::default().spawn_local(async move {
                let fname = wpath_bg
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("обои")
                    .to_string();
                let res = gio::spawn_blocking(move || apply_wallpaper(&wpath_bg))
                    .await
                    .unwrap_or_else(|e| Err(format!("Task panicked: {e:?}")));
                match res {
                    Ok(()) => status_cb.ok(&format!("Обои '{fname}' применены")),
                    Err(e) => status_cb.error(&e),
                }
            });
        });
    }

    wall_scroll.set_child(Some(&wall_strip));
    wall_card.append(&wall_scroll);
    content.append(&wall_card);

    // 3. Keyboard, Touchpad & Autolock Overrides Card
    let ovr_card = Box::new(Orientation::Vertical, 10);
    ovr_card.add_css_class("setup-card");

    let ovr_title = Label::new(Some("Клавиатура, тачпад и автоблокировка (локальные overrides)"));
    ovr_title.add_css_class("card-title");
    ovr_title.set_xalign(0.0);
    ovr_card.append(&ovr_title);

    let ovr_sub = Label::new(Some(
        "Сохраняется в inputs_local.lua и hypridle_local.conf, не загрязняя Git",
    ));
    ovr_sub.add_css_class("card-muted");
    ovr_sub.set_xalign(0.0);
    ovr_card.append(&ovr_sub);

    let saved_inputs = std::fs::read_to_string(paths.inputs_local_path())
        .map(|s| parse_inputs_lua(&s))
        .unwrap_or_default();
    let saved_idle = std::fs::read_to_string(paths.hypridle_local_conf_path())
        .map(|s| parse_hypridle_conf(&s))
        .unwrap_or_default();

    // Keyboard row
    let kb_row = Box::new(Orientation::Horizontal, 10);
    let kb_layout_box = Box::new(Orientation::Vertical, 4);
    kb_layout_box.set_hexpand(true);
    let kb_lbl = Label::new(Some("Раскладки (kb_layout)"));
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

    // Layout switch shortcut
    let opt_box = Box::new(Orientation::Vertical, 4);
    let opt_lbl = Label::new(Some("Переключение раскладки"));
    opt_lbl.add_css_class("card-muted");
    opt_lbl.set_xalign(0.0);
    opt_box.append(&opt_lbl);

    let opt_labels: Vec<&str> = KB_TOGGLE_OPTIONS.iter().map(|(_, lbl)| *lbl).collect();
    let opt_dd = DropDown::new(Some(StringList::new(&opt_labels)), gtk4::Expression::NONE);
    opt_dd.add_css_class("setup-dropdown");
    let opt_idx = KB_TOGGLE_OPTIONS
        .iter()
        .position(|(val, _)| *val == saved_inputs.kb_options)
        .unwrap_or(0);
    opt_dd.set_selected(opt_idx as u32);
    opt_box.append(&opt_dd);
    kb_row.append(&opt_box);
    ovr_card.append(&kb_row);

    // Touchpad toggles
    let tp_row = Box::new(Orientation::Horizontal, 16);
    tp_row.add_css_class("setup-subcard");

    let sw_natural = make_switch_row(&tp_row, "Естественная прокрутка", saved_inputs.natural_scroll);
    let sw_tap = make_switch_row(&tp_row, "Тап для клика", saved_inputs.tap_to_click);
    let sw_dwt = make_switch_row(
        &tp_row,
        "Блокировать при печати",
        saved_inputs.disable_while_typing,
    );
    ovr_card.append(&tp_row);

    // Autolock row
    let idle_row = Box::new(Orientation::Horizontal, 12);
    idle_row.add_css_class("setup-subcard");
    let sw_idle = make_switch_row(&idle_row, "Автоблокировка (hypridle)", saved_idle.enabled);

    let idle_labels: Vec<&str> = IDLE_PRESETS.iter().map(|(l, _, _, _)| *l).collect();
    let idle_dd = DropDown::new(Some(StringList::new(&idle_labels)), gtk4::Expression::NONE);
    idle_dd.add_css_class("setup-dropdown");
    idle_dd.set_hexpand(true);
    let idle_idx = IDLE_PRESETS
        .iter()
        .position(|&(_, d, l, dp)| {
            d == saved_idle.dim_secs && l == saved_idle.lock_secs && dp == saved_idle.dpms_secs
        })
        .unwrap_or(1);
    idle_dd.set_selected(idle_idx as u32);
    idle_dd.set_sensitive(saved_idle.enabled);
    idle_row.append(&idle_dd);

    {
        let idle_dd_c = idle_dd.clone();
        sw_idle.connect_state_set(move |_, active| {
            idle_dd_c.set_sensitive(active);
            glib::Propagation::Proceed
        });
    }
    ovr_card.append(&idle_row);

    // Action buttons for overrides
    let ovr_actions = Box::new(Orientation::Horizontal, 8);
    let ovr_reset_btn = Button::with_label("Сбросить overrides");
    ovr_reset_btn.add_css_class("btn-danger");
    ovr_actions.append(&ovr_reset_btn);

    let spacer = Box::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    ovr_actions.append(&spacer);

    let ovr_save_btn = Button::with_label("Сохранить overrides");
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
        let sw_natural = sw_natural.clone();
        let sw_tap = sw_tap.clone();
        let sw_dwt = sw_dwt.clone();
        let sw_idle = sw_idle.clone();
        let idle_dd = idle_dd.clone();

        ovr_save_btn.connect_clicked(move |_| {
            let layout = kb_entry.text().trim().to_string();
            if layout.is_empty() {
                status_cb.error("Поле kb_layout не может быть пустым");
                return;
            }
            let opt_idx = opt_dd.selected() as usize;
            let kb_options = KB_TOGGLE_OPTIONS
                .get(opt_idx)
                .map(|(v, _)| *v)
                .unwrap_or("grp:alt_shift_toggle")
                .to_string();

            let input_cfg = InputOverrides {
                kb_layout: layout,
                kb_options,
                natural_scroll: sw_natural.is_active(),
                tap_to_click: sw_tap.is_active(),
                disable_while_typing: sw_dwt.is_active(),
            };

            let idle_idx = idle_dd.selected() as usize;
            let &(_, dim_secs, lock_secs, dpms_secs) =
                IDLE_PRESETS.get(idle_idx).unwrap_or(&IDLE_PRESETS[1]);
            let idle_cfg = IdleConfig {
                enabled: sw_idle.is_active(),
                dim_secs,
                lock_secs,
                dpms_secs,
            };

            if let Err(e) = save_inputs_and_reload(&paths, &input_cfg) {
                status_cb.error(&e);
                return;
            }
            if let Err(e) = apply_hypridle(&paths, &idle_cfg) {
                status_cb.warn(&format!("Ввод применён, но hypridle: {e}"));
                return;
            }
            status_cb.ok("Overrides клавиатуры, тачпада и hypridle применены");
        });
    }

    // Wire Reset Overrides
    {
        let paths = paths.clone();
        let status_cb = status_cb.clone();
        let kb_entry = kb_entry.clone();
        let opt_dd = opt_dd.clone();
        let sw_natural = sw_natural.clone();
        let sw_tap = sw_tap.clone();
        let sw_dwt = sw_dwt.clone();
        let sw_idle = sw_idle.clone();
        let idle_dd = idle_dd.clone();

        ovr_reset_btn.connect_clicked(move |_| match reset_interface_overrides(&paths) {
            Ok(()) => {
                let def_in = InputOverrides::default();
                kb_entry.set_text(&def_in.kb_layout);
                opt_dd.set_selected(0);
                sw_natural.set_active(def_in.natural_scroll);
                sw_tap.set_active(def_in.tap_to_click);
                sw_dwt.set_active(def_in.disable_while_typing);
                sw_idle.set_active(true);
                idle_dd.set_selected(1);
                idle_dd.set_sensitive(true);
                status_cb.ok("Локальные overrides сброшены на базовый конфиг");
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
