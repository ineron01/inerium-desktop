use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Align, ApplicationWindow, Box, Button, DropDown, Entry, Label, Orientation, PolicyType,
    ScrolledWindow, StringList, Switch,
};

use crate::i18n::{Lang, tr};
use crate::model::{
    HyprMonitor, LayoutPreset, MonitorConfig, SetupPaths, apply_layout_preset, format_scale,
    merge_monitors, parse_monitors_lua, read_workspaces_count, write_workspaces_count,
};
use crate::system::{
    hyprctl_reload, is_managed_monitors_overlay, query_hypr_monitors, reset_monitors_to_auto,
    save_and_reload_monitors,
};
use crate::ui::StatusCallback;

#[derive(Clone, Debug)]
pub struct ModeMapping {
    pub resolution: String,
    pub refresh_rates: Vec<String>,
}

fn group_available_modes(modes: &[String]) -> Vec<ModeMapping> {
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for m in modes {
        if m == "preferred" {
            continue;
        }
        if let Some((res, hz)) = m.split_once('@') {
            let entry = map.entry(res.to_string()).or_default();
            if !entry.contains(&hz.to_string()) {
                entry.push(hz.to_string());
            }
        } else {
            let entry = map.entry(m.clone()).or_default();
            if !entry.contains(&"Auto".to_string()) {
                entry.push("Auto".to_string());
            }
        }
    }
    let mut result = vec![ModeMapping {
        resolution: "preferred".to_string(),
        refresh_rates: vec!["Auto".to_string()],
    }];
    for (res, mut hzs) in map.into_iter().rev() {
        hzs.sort_by(|a, b| {
            let num_a = a.trim_end_matches("Hz").parse::<f64>().unwrap_or(0.0);
            let num_b = b.trim_end_matches("Hz").parse::<f64>().unwrap_or(0.0);
            num_b.total_cmp(&num_a)
        });
        result.push(ModeMapping {
            resolution: res,
            refresh_rates: hzs,
        });
    }
    result
}

pub fn clamp_window_to_screen(win: &ApplicationWindow) {
    if let Some(display) = gdk4::Display::default() {
        let monitors = display.monitors();
        if let Some(mon) = monitors.item(0).and_then(|m| m.downcast::<gdk4::Monitor>().ok()) {
            let geom = mon.geometry();
            let max_w = (geom.width() - 32).max(460);
            let max_h = (geom.height() - 48).max(380);
            let target_w = 840.min(max_w);
            let target_h = 560.min(max_h);
            win.set_default_size(target_w, target_h);
        }
    }
}

struct MonitorRowWidgets {
    output: String,
    modes: Vec<ModeMapping>,
    res_dd: DropDown,
    hz_dd: DropDown,
    current_hz_options: Rc<RefCell<Vec<String>>>,
    scale_options: Vec<f64>,
    scale_dd: DropDown,
    pos_entry: Entry,
    vrr_switch: Switch,
}

pub fn build_displays_tab(
    win: ApplicationWindow,
    paths: SetupPaths,
    status_cb: StatusCallback,
    lang: Lang,
) -> ScrolledWindow {
    let scroll = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .vscrollbar_policy(PolicyType::Automatic)
        .vexpand(true)
        .hexpand(true)
        .build();

    let root = Box::new(Orientation::Vertical, 12);
    root.set_margin_end(4);

    // 1. Workspaces per monitor card
    let ws_card = Box::new(Orientation::Vertical, 8);
    ws_card.add_css_class("setup-card");

    let ws_head = Box::new(Orientation::Horizontal, 8);
    let ws_title_box = Box::new(Orientation::Vertical, 2);
    ws_title_box.set_hexpand(true);

    let ws_title = Label::new(Some(tr(
        lang,
        "Workspaces per monitor",
        "Рабочие столы на монитор",
    )));
    ws_title.add_css_class("card-title");
    ws_title.set_xalign(0.0);
    ws_title_box.append(&ws_title);

    let ws_sub = Label::new(Some(tr(
        lang,
        "Saved to ~/.config/hypr/config/workspaces_count and applied immediately",
        "Записывается в ~/.config/hypr/config/workspaces_count и сразу применяется",
    )));
    ws_sub.add_css_class("card-muted");
    ws_sub.set_xalign(0.0);
    ws_title_box.append(&ws_sub);
    ws_head.append(&ws_title_box);

    let current_ws = Rc::new(RefCell::new(read_workspaces_count(
        &paths.workspaces_count_path(),
    )));
    let ws_buttons: Rc<RefCell<Vec<(u8, Button)>>> = Rc::new(RefCell::new(Vec::new()));

    let seg = Box::new(Orientation::Horizontal, 2);
    seg.add_css_class("seg-group");
    for n in 1u8..=9 {
        let btn = Button::with_label(&n.to_string());
        btn.add_css_class("seg-btn");
        if n == *current_ws.borrow() {
            btn.add_css_class("active");
        }
        ws_buttons.borrow_mut().push((n, btn.clone()));
        seg.append(&btn);
    }
    ws_head.append(&seg);
    ws_card.append(&ws_head);
    root.append(&ws_card);

    for (n, btn) in ws_buttons.borrow().iter() {
        let n = *n;
        let current_ws = current_ws.clone();
        let ws_buttons = ws_buttons.clone();
        let paths = paths.clone();
        let status_cb = status_cb.clone();
        btn.connect_clicked(move |_| {
            if let Err(e) = write_workspaces_count(&paths.workspaces_count_path(), n) {
                status_cb.error(&e);
                return;
            }
            *current_ws.borrow_mut() = n;
            for (val, b) in ws_buttons.borrow().iter() {
                if *val == n {
                    b.add_css_class("active");
                } else {
                    b.remove_css_class("active");
                }
            }
            match hyprctl_reload() {
                Ok(()) => status_cb.ok(&match lang {
                    Lang::En => format!("Workspaces: {n} per monitor (applied)"),
                    Lang::Ru => format!("Воркспейсы: {n} на монитор (применено)"),
                }),
                Err(e) => status_cb.warn(&match lang {
                    Lang::En => format!("Saved ({n}), but hyprctl reload: {e}"),
                    Lang::Ru => format!("Сохранено ({n}), но hyprctl reload: {e}"),
                }),
            }
        });
    }

    // 2. Monitors card
    let mon_card = Box::new(Orientation::Vertical, 10);
    mon_card.add_css_class("setup-card");
    mon_card.set_vexpand(true);

    let mon_head = Box::new(Orientation::Horizontal, 8);
    let mon_title_box = Box::new(Orientation::Vertical, 2);
    mon_title_box.set_hexpand(true);

    let mon_title = Label::new(Some(tr(
        lang,
        "Connected displays",
        "Подключённые мониторы",
    )));
    mon_title.add_css_class("card-title");
    mon_title.set_xalign(0.0);
    mon_title_box.append(&mon_title);

    let managed_overlay = is_managed_monitors_overlay(&paths);
    let mon_sub_text = if managed_overlay {
        tr(
            lang,
            "Managed by overlay (monitors_local.lua is a symlink / workspace rules), overwriting disabled",
            "Управляется оверлеем (monitors_local.lua — симлинк / правила воркспейсов), перезапись отключена",
        )
    } else {
        tr(
            lang,
            "Saved to ~/.config/hypr/config/monitors_local.lua with luac -p validation",
            "Сохраняется в ~/.config/hypr/config/monitors_local.lua с проверкой luac -p",
        )
    };
    let mon_sub = Label::new(Some(mon_sub_text));
    mon_sub.add_css_class("card-muted");
    mon_sub.set_xalign(0.0);
    mon_title_box.append(&mon_sub);
    mon_head.append(&mon_title_box);

    let refresh_btn = Button::with_label(tr(lang, "󰑐 Refresh", "󰑐 Обновить"));
    refresh_btn.add_css_class("btn-secondary");
    mon_head.append(&refresh_btn);
    mon_card.append(&mon_head);

    // Layout preset bar
    let preset_row = Box::new(Orientation::Horizontal, 6);
    let preset_lbl = Label::new(Some(tr(lang, "Layout:", "Расположение:")));
    preset_lbl.add_css_class("card-meta");
    preset_row.append(&preset_lbl);

    let btn_auto = Button::with_label(tr(lang, "Auto", "Авто"));
    let btn_htop = Button::with_label(tr(lang, "Side by side (top)", "Рядом (верх)"));
    let btn_hbot = Button::with_label(tr(lang, "Side by side (bottom)", "Рядом (низ)"));
    let btn_vert = Button::with_label(tr(lang, "Vertical", "Вертикально"));
    for b in [&btn_auto, &btn_htop, &btn_hbot, &btn_vert] {
        b.add_css_class("btn-secondary");
        preset_row.append(b);
    }
    mon_card.append(&preset_row);

    // Visual preview strip
    let preview_box = Box::new(Orientation::Horizontal, 8);
    preview_box.add_css_class("layout-preview");
    mon_card.append(&preview_box);

    // Scrollable monitor cards container
    let list_box = Box::new(Orientation::Vertical, 8);
    mon_card.append(&list_box);

    // Bottom action row for monitors
    let action_row = Box::new(Orientation::Horizontal, 8);
    let reset_btn = Button::with_label(tr(
        lang,
        "Reset to auto-detect",
        "Сбросить на автоопределение",
    ));
    reset_btn.add_css_class("btn-danger");
    if managed_overlay {
        reset_btn.set_sensitive(false);
    }
    action_row.append(&reset_btn);

    let spacer = Box::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    action_row.append(&spacer);

    let save_btn = Button::with_label(tr(lang, "Save & Apply", "Сохранить и применить"));
    save_btn.add_css_class("btn-primary");
    if managed_overlay {
        save_btn.set_sensitive(false);
    }
    action_row.append(&save_btn);
    mon_card.append(&action_row);

    root.append(&mon_card);

    let live_monitors: Rc<RefCell<Vec<HyprMonitor>>> = Rc::new(RefCell::new(Vec::new()));
    let row_widgets: Rc<RefCell<Vec<MonitorRowWidgets>>> = Rc::new(RefCell::new(Vec::new()));

    let read_current_configs = {
        let row_widgets = row_widgets.clone();
        move || -> Vec<MonitorConfig> {
            row_widgets
                .borrow()
                .iter()
                .map(|w| {
                    let res_idx = w.res_dd.selected() as usize;
                    let res = w
                        .modes
                        .get(res_idx)
                        .map(|m| m.resolution.as_str())
                        .unwrap_or("preferred");
                    let mode = if res == "preferred" {
                        "preferred".to_string()
                    } else {
                        let hz_idx = w.hz_dd.selected() as usize;
                        let hz_opts = w.current_hz_options.borrow();
                        if let Some(hz) = hz_opts.get(hz_idx) {
                            if hz == "Auto" {
                                res.to_string()
                            } else {
                                format!("{res}@{hz}")
                            }
                        } else {
                            res.to_string()
                        }
                    };

                    let scale_idx = w.scale_dd.selected() as usize;
                    let scale = w.scale_options.get(scale_idx).copied().unwrap_or(1.0);
                    let pos_raw = w.pos_entry.text().trim().to_string();
                    let position = if pos_raw.is_empty() {
                        "auto".to_string()
                    } else {
                        pos_raw
                    };
                    MonitorConfig {
                        output: w.output.clone(),
                        mode,
                        position,
                        scale,
                        vrr: w.vrr_switch.is_active(),
                    }
                })
                .collect()
        }
    };

    let rebuild_preview = {
        let preview_box = preview_box.clone();
        let live_monitors = live_monitors.clone();
        move |configs: &[MonitorConfig]| {
            while let Some(child) = preview_box.first_child() {
                preview_box.remove(&child);
            }
            if configs.is_empty() {
                let lbl = Label::new(Some(tr(
                    lang,
                    "No displays detected (no response from hyprctl)",
                    "Мониторы не обнаружены (нет ответа hyprctl)",
                )));
                lbl.add_css_class("card-muted");
                preview_box.append(&lbl);
                return;
            }
            let live = live_monitors.borrow();
            for cfg in configs {
                let card = Box::new(Orientation::Vertical, 2);
                card.add_css_class("mon-box");
                if live
                    .iter()
                    .find(|m| m.name == cfg.output)
                    .is_some_and(|m| m.focused)
                {
                    card.add_css_class("focused");
                }
                let title = Label::new(Some(&cfg.output));
                title.add_css_class("card-title");
                title.set_xalign(0.0);
                card.append(&title);

                let details = Label::new(Some(&format!(
                    "{} · ×{} · {}",
                    cfg.mode,
                    format_scale(cfg.scale),
                    cfg.position
                )));
                details.add_css_class("card-muted");
                details.set_xalign(0.0);
                card.append(&details);
                preview_box.append(&card);
            }
        }
    };

    let win_populate = win.clone();
    let populate = {
        let list_box = list_box.clone();
        let live_monitors = live_monitors.clone();
        let row_widgets = row_widgets.clone();
        let paths = paths.clone();
        let status_cb = status_cb.clone();
        let rebuild_preview = rebuild_preview.clone();
        let read_current_configs = read_current_configs.clone();
        let win = win_populate;

        Rc::new(move || {
            while let Some(child) = list_box.first_child() {
                list_box.remove(&child);
            }
            row_widgets.borrow_mut().clear();

            let live = match query_hypr_monitors() {
                Ok(m) => m,
                Err(e) => {
                    status_cb.warn(&e);
                    Vec::new()
                }
            };
            *live_monitors.borrow_mut() = live.clone();

            let saved = std::fs::read_to_string(paths.monitors_local_path())
                .map(|s| parse_monitors_lua(&s))
                .unwrap_or_default();
            let configs = merge_monitors(&live, &saved);

            if configs.is_empty() {
                let empty = Label::new(Some(tr(
                    lang,
                    "Could not get display list from hyprctl monitors -j",
                    "Не удалось получить список мониторов от hyprctl monitors -j",
                )));
                empty.add_css_class("card-muted");
                empty.set_margin_top(18);
                list_box.append(&empty);
                rebuild_preview(&[]);
                return;
            }

            for cfg in &configs {
                let live_m = live.iter().find(|m| m.name == cfg.output);
                let card = Box::new(Orientation::Vertical, 8);
                card.add_css_class("setup-subcard");

                // Card header
                let hdr = Box::new(Orientation::Horizontal, 8);
                let name_lbl = Label::new(Some(&cfg.output));
                name_lbl.add_css_class("card-title");
                name_lbl.set_xalign(0.0);
                hdr.append(&name_lbl);

                if let Some(lm) = live_m {
                    let desc = Label::new(Some(&format!(
                        "{} ({}x{}@{:.0}Hz)",
                        lm.description, lm.width, lm.height, lm.refresh_rate
                    )));
                    desc.add_css_class("card-muted");
                    desc.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                    desc.set_hexpand(true);
                    desc.set_xalign(0.0);
                    hdr.append(&desc);

                    if lm.focused {
                        let pill = Label::new(Some(tr(lang, "Active", "Активный")));
                        pill.add_css_class("pill");
                        pill.add_css_class("pill-ok");
                        hdr.append(&pill);
                    }
                }
                card.append(&hdr);

                // Controls grid row
                let controls = Box::new(Orientation::Horizontal, 10);

                // Split Mode into Resolution and Refresh Rate dropdowns
                let mut raw_modes = vec!["preferred".to_string()];
                if let Some(lm) = live_m {
                    for m in &lm.available_modes {
                        if !raw_modes.contains(m) {
                            raw_modes.push(m.clone());
                        }
                    }
                }
                if !raw_modes.contains(&cfg.mode) {
                    raw_modes.push(cfg.mode.clone());
                }

                let mode_mappings = group_available_modes(&raw_modes);
                let res_options: Vec<String> = mode_mappings.iter().map(|m| m.resolution.clone()).collect();
                let res_labels: Vec<String> = res_options.iter().map(|r| {
                    if r == "preferred" {
                        tr(lang, "preferred (Auto)", "preferred (Авто)").to_string()
                    } else {
                        r.clone()
                    }
                }).collect();
                let res_refs: Vec<&str> = res_labels.iter().map(|s| s.as_str()).collect();

                let (curr_res, curr_hz) = if cfg.mode == "preferred" {
                    ("preferred".to_string(), "Auto".to_string())
                } else if let Some((r, h)) = cfg.mode.split_once('@') {
                    (r.to_string(), h.to_string())
                } else {
                    (cfg.mode.clone(), "Auto".to_string())
                };

                let res_idx = res_options.iter().position(|r| r == &curr_res).unwrap_or(0);
                let res_dd = DropDown::new(Some(StringList::new(&res_refs)), gtk4::Expression::NONE);
                res_dd.add_css_class("setup-dropdown");
                res_dd.set_selected(res_idx as u32);
                res_dd.set_hexpand(true);

                let current_hz_options = Rc::new(RefCell::new(
                    mode_mappings.get(res_idx).map(|m| m.refresh_rates.clone()).unwrap_or_else(|| vec!["Auto".to_string()])
                ));

                let make_hz_labels = move |hzs: &[String]| -> Vec<String> {
                    hzs.iter().map(|h| {
                        if h == "Auto" {
                            tr(lang, "Auto Hz", "Авто Гц").to_string()
                        } else {
                            let num = h.trim_end_matches("Hz").split('.').next().unwrap_or(h);
                            format!("{num} Hz ({h})")
                        }
                    }).collect()
                };

                let initial_hzs = current_hz_options.borrow().clone();
                let hz_labels = make_hz_labels(&initial_hzs);
                let hz_refs: Vec<&str> = hz_labels.iter().map(|s| s.as_str()).collect();
                let hz_dd = DropDown::new(Some(StringList::new(&hz_refs)), gtk4::Expression::NONE);
                hz_dd.add_css_class("setup-dropdown");
                let hz_idx = initial_hzs.iter().position(|h| h == &curr_hz).unwrap_or(0);
                hz_dd.set_selected(hz_idx as u32);
                hz_dd.set_sensitive(curr_res != "preferred");

                // Wire dynamic refresh rate filtering on resolution change
                {
                    let mode_mappings = mode_mappings.clone();
                    let hz_dd = hz_dd.clone();
                    let current_hz_options = current_hz_options.clone();
                    let rebuild_preview_c = rebuild_preview.clone();
                    let read_c = read_current_configs.clone();
                    res_dd.connect_selected_notify(move |dd| {
                        let sel_res_idx = dd.selected() as usize;
                        let mapping = mode_mappings.get(sel_res_idx);
                        let new_hzs = mapping.map(|m| m.refresh_rates.clone()).unwrap_or_else(|| vec!["Auto".to_string()]);
                        *current_hz_options.borrow_mut() = new_hzs.clone();
                        let labels = make_hz_labels(&new_hzs);
                        let refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
                        hz_dd.set_model(Some(&StringList::new(&refs)));
                        hz_dd.set_selected(0);
                        let is_pref = mapping.map(|m| m.resolution == "preferred").unwrap_or(false);
                        hz_dd.set_sensitive(!is_pref);
                        rebuild_preview_c(&read_c());
                    });
                }

                controls.append(&labeled_field(tr(lang, "Resolution", "Разрешение"), &res_dd));
                controls.append(&labeled_field(tr(lang, "Refresh rate", "Частота"), &hz_dd));

                // Scale dropdown
                let mut scale_options = vec![1.0, 1.25, 1.5, 1.75, 2.0];
                if !scale_options
                    .iter()
                    .any(|&s| (s - cfg.scale).abs() < 0.01)
                {
                    scale_options.push(cfg.scale);
                    scale_options.sort_by(|a, b| a.total_cmp(b));
                }
                let scale_labels: Vec<String> =
                    scale_options.iter().map(|&s| format_scale(s)).collect();
                let scale_refs: Vec<&str> = scale_labels.iter().map(|s| s.as_str()).collect();
                let scale_dd = DropDown::new(Some(StringList::new(&scale_refs)), gtk4::Expression::NONE);
                scale_dd.add_css_class("setup-dropdown");
                let scale_idx = scale_options
                    .iter()
                    .position(|&s| (s - cfg.scale).abs() < 0.01)
                    .unwrap_or(0);
                scale_dd.set_selected(scale_idx as u32);
                controls.append(&labeled_field(tr(lang, "Scale", "Масштаб"), &scale_dd));

                // Position entry
                let pos_entry = Entry::new();
                pos_entry.add_css_class("setup-entry");
                pos_entry.set_width_chars(10);
                pos_entry.set_text(&cfg.position);
                controls.append(&labeled_field(tr(lang, "Position (XxY)", "Позиция (XxY)"), &pos_entry));

                // VRR switch
                let vrr_switch = Switch::new();
                vrr_switch.add_css_class("setup-switch");
                vrr_switch.set_active(cfg.vrr);
                vrr_switch.set_valign(Align::Center);
                controls.append(&labeled_field("VRR", &vrr_switch));

                card.append(&controls);
                list_box.append(&card);

                // Keep preview live when editing
                let rebuild_preview_c = rebuild_preview.clone();
                let read_c = read_current_configs.clone();
                hz_dd.connect_selected_notify(move |_| rebuild_preview_c(&read_c()));

                let rebuild_preview_c = rebuild_preview.clone();
                let read_c = read_current_configs.clone();
                let win_c = win.clone();
                scale_dd.connect_selected_notify(move |_| {
                    rebuild_preview_c(&read_c());
                    clamp_window_to_screen(&win_c);
                });

                let rebuild_preview_c = rebuild_preview.clone();
                let read_c = read_current_configs.clone();
                pos_entry.connect_changed(move |_| rebuild_preview_c(&read_c()));

                row_widgets.borrow_mut().push(MonitorRowWidgets {
                    output: cfg.output.clone(),
                    modes: mode_mappings,
                    res_dd,
                    hz_dd,
                    current_hz_options,
                    scale_options,
                    scale_dd,
                    pos_entry,
                    vrr_switch,
                });
            }

            rebuild_preview(&configs);
        })
    };

    populate();

    // Preset click wiring
    for (btn, preset) in [
        (btn_auto, LayoutPreset::Auto),
        (btn_htop, LayoutPreset::HorizontalTop),
        (btn_hbot, LayoutPreset::HorizontalBottom),
        (btn_vert, LayoutPreset::Vertical),
    ] {
        let read_current_configs = read_current_configs.clone();
        let live_monitors = live_monitors.clone();
        let row_widgets = row_widgets.clone();
        let rebuild_preview = rebuild_preview.clone();
        btn.connect_clicked(move |_| {
            let mut configs = read_current_configs();
            apply_layout_preset(&mut configs, &live_monitors.borrow(), preset);
            for (w, cfg) in row_widgets.borrow().iter().zip(configs.iter()) {
                w.pos_entry.set_text(&cfg.position);
            }
            rebuild_preview(&configs);
        });
    }

    // Refresh
    {
        let populate = populate.clone();
        let status_cb = status_cb.clone();
        refresh_btn.connect_clicked(move |_| {
            populate();
            status_cb.ok(tr(lang, "Display list refreshed", "Список мониторов обновлён"));
        });
    }

    // Save & Apply
    {
        let read_current_configs = read_current_configs.clone();
        let paths = paths.clone();
        let status_cb = status_cb.clone();
        let populate = populate.clone();
        let win_c = win.clone();
        save_btn.connect_clicked(move |_| {
            let configs = read_current_configs();
            if configs.is_empty() {
                status_cb.warn(tr(lang, "No displays to save", "Нет мониторов для сохранения"));
                return;
            }
            match save_and_reload_monitors(&paths, &configs) {
                Ok(()) => {
                    clamp_window_to_screen(&win_c);
                    status_cb.ok(tr(
                        lang,
                        "monitors_local.lua verified (luac -p) and applied",
                        "monitors_local.lua проверен (luac -p) и применён",
                    ));
                    populate();
                }
                Err(e) => status_cb.error(&e),
            }
        });
    }

    // Reset to auto-detection
    {
        let paths = paths.clone();
        let status_cb = status_cb.clone();
        let populate = populate.clone();
        let win_c = win.clone();
        reset_btn.connect_clicked(move |_| match reset_monitors_to_auto(&paths) {
            Ok(()) => {
                clamp_window_to_screen(&win_c);
                status_cb.ok(tr(
                    lang,
                    "Display config reset to auto-detect",
                    "Конфиг мониторов сброшен на автоопределение",
                ));
                populate();
            }
            Err(e) => status_cb.error(&e),
        });
    }

    scroll.set_child(Some(&root));
    scroll
}

fn labeled_field(label: &str, widget: &impl IsA<gtk4::Widget>) -> Box {
    let bx = Box::new(Orientation::Vertical, 4);
    let lbl = Label::new(Some(label));
    lbl.add_css_class("card-muted");
    lbl.set_xalign(0.0);
    bx.append(&lbl);
    bx.append(widget);
    bx
}
