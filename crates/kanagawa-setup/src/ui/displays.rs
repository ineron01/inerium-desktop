use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Align, Box, Button, DropDown, Entry, Label, Orientation, PolicyType, ScrolledWindow,
    StringList, Switch,
};

use crate::model::{
    HyprMonitor, LayoutPreset, MonitorConfig, SetupPaths, apply_layout_preset, format_scale,
    merge_monitors, parse_monitors_lua, read_workspaces_count, write_workspaces_count,
};
use crate::system::{
    hyprctl_reload, is_managed_monitors_overlay, query_hypr_monitors, reset_monitors_to_auto,
    save_and_reload_monitors,
};
use crate::ui::StatusCallback;

struct MonitorRowWidgets {
    output: String,
    mode_options: Vec<String>,
    mode_dd: DropDown,
    scale_options: Vec<f64>,
    scale_dd: DropDown,
    pos_entry: Entry,
    vrr_switch: Switch,
}

pub fn build_displays_tab(paths: SetupPaths, status_cb: StatusCallback) -> Box {
    let root = Box::new(Orientation::Vertical, 12);

    // 1. Workspaces per monitor card
    let ws_card = Box::new(Orientation::Vertical, 8);
    ws_card.add_css_class("setup-card");

    let ws_head = Box::new(Orientation::Horizontal, 8);
    let ws_title_box = Box::new(Orientation::Vertical, 2);
    ws_title_box.set_hexpand(true);

    let ws_title = Label::new(Some("Рабочие столы на монитор"));
    ws_title.add_css_class("card-title");
    ws_title.set_xalign(0.0);
    ws_title_box.append(&ws_title);

    let ws_sub = Label::new(Some(
        "Записывается в ~/.config/hypr/config/workspaces_count и сразу применяется",
    ));
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
                Ok(()) => status_cb.ok(&format!("Воркспейсы: {n} на монитор (применено)")),
                Err(e) => status_cb.warn(&format!("Сохранено ({n}), но hyprctl reload: {e}")),
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

    let mon_title = Label::new(Some("Подключённые мониторы"));
    mon_title.add_css_class("card-title");
    mon_title.set_xalign(0.0);
    mon_title_box.append(&mon_title);

    let managed_overlay = is_managed_monitors_overlay(&paths);
    let mon_sub_text = if managed_overlay {
        "Управляется оверлеем (monitors_local.lua — симлинк / правила воркспейсов), перезапись отключена"
    } else {
        "Сохраняется в ~/.config/hypr/config/monitors_local.lua с проверкой luac -p"
    };
    let mon_sub = Label::new(Some(mon_sub_text));
    mon_sub.add_css_class("card-muted");
    mon_sub.set_xalign(0.0);
    mon_title_box.append(&mon_sub);
    mon_head.append(&mon_title_box);

    let refresh_btn = Button::with_label("󰑐 Обновить");
    refresh_btn.add_css_class("btn-secondary");
    mon_head.append(&refresh_btn);
    mon_card.append(&mon_head);

    // Layout preset bar
    let preset_row = Box::new(Orientation::Horizontal, 6);
    let preset_lbl = Label::new(Some("Расположение:"));
    preset_lbl.add_css_class("card-meta");
    preset_row.append(&preset_lbl);

    let btn_auto = Button::with_label("Авто");
    let btn_htop = Button::with_label("Рядом (верх)");
    let btn_hbot = Button::with_label("Рядом (низ)");
    let btn_vert = Button::with_label("Вертикально");
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
    let scroll = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .vscrollbar_policy(PolicyType::Automatic)
        .vexpand(true)
        .build();
    let list_box = Box::new(Orientation::Vertical, 8);
    scroll.set_child(Some(&list_box));
    mon_card.append(&scroll);

    // Bottom action row for monitors
    let action_row = Box::new(Orientation::Horizontal, 8);
    let reset_btn = Button::with_label("Сбросить на автоопределение");
    reset_btn.add_css_class("btn-danger");
    if managed_overlay {
        reset_btn.set_sensitive(false);
    }
    action_row.append(&reset_btn);

    let spacer = Box::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    action_row.append(&spacer);

    let save_btn = Button::with_label("Сохранить и применить");
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
                    let mode_idx = w.mode_dd.selected() as usize;
                    let mode = w_mode_at(&w.mode_options, mode_idx);
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
                let lbl = Label::new(Some("Мониторы не обнаружены (нет ответа hyprctl)"));
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

    let populate = {
        let list_box = list_box.clone();
        let live_monitors = live_monitors.clone();
        let row_widgets = row_widgets.clone();
        let paths = paths.clone();
        let status_cb = status_cb.clone();
        let rebuild_preview = rebuild_preview.clone();
        let read_current_configs = read_current_configs.clone();

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
                let empty = Label::new(Some(
                    "Не удалось получить список мониторов от hyprctl monitors -j",
                ));
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
                        let pill = Label::new(Some("Активный"));
                        pill.add_css_class("pill");
                        pill.add_css_class("pill-ok");
                        hdr.append(&pill);
                    }
                }
                card.append(&hdr);

                // Controls grid row
                let controls = Box::new(Orientation::Horizontal, 10);

                // Mode dropdown
                let mut mode_options = vec!["preferred".to_string()];
                if let Some(lm) = live_m {
                    for m in &lm.available_modes {
                        if !mode_options.contains(m) {
                            mode_options.push(m.clone());
                        }
                    }
                }
                if !mode_options.contains(&cfg.mode) {
                    mode_options.push(cfg.mode.clone());
                }
                let mode_refs: Vec<&str> = mode_options.iter().map(|s| s.as_str()).collect();
                let mode_model = StringList::new(&mode_refs);
                let mode_dd = DropDown::new(Some(mode_model), gtk4::Expression::NONE);
                mode_dd.add_css_class("setup-dropdown");
                mode_dd.set_hexpand(true);
                let mode_idx = mode_options
                    .iter()
                    .position(|m| m == &cfg.mode)
                    .unwrap_or(0);
                mode_dd.set_selected(mode_idx as u32);

                let mode_box = labeled_field("Режим / Гц", &mode_dd);
                mode_box.set_hexpand(true);
                controls.append(&mode_box);

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
                controls.append(&labeled_field("Масштаб", &scale_dd));

                // Position entry
                let pos_entry = Entry::new();
                pos_entry.add_css_class("setup-entry");
                pos_entry.set_width_chars(10);
                pos_entry.set_text(&cfg.position);
                controls.append(&labeled_field("Позиция (XxY)", &pos_entry));

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
                mode_dd.connect_selected_notify(move |_| rebuild_preview_c(&read_c()));

                let rebuild_preview_c = rebuild_preview.clone();
                let read_c = read_current_configs.clone();
                scale_dd.connect_selected_notify(move |_| rebuild_preview_c(&read_c()));

                let rebuild_preview_c = rebuild_preview.clone();
                let read_c = read_current_configs.clone();
                pos_entry.connect_changed(move |_| rebuild_preview_c(&read_c()));

                row_widgets.borrow_mut().push(MonitorRowWidgets {
                    output: cfg.output.clone(),
                    mode_options,
                    mode_dd,
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
            status_cb.ok("Список мониторов обновлён");
        });
    }

    // Save & Apply
    {
        let read_current_configs = read_current_configs.clone();
        let paths = paths.clone();
        let status_cb = status_cb.clone();
        let populate = populate.clone();
        save_btn.connect_clicked(move |_| {
            let configs = read_current_configs();
            if configs.is_empty() {
                status_cb.warn("Нет мониторов для сохранения");
                return;
            }
            match save_and_reload_monitors(&paths, &configs) {
                Ok(()) => {
                    status_cb.ok("monitors_local.lua проверен (luac -p) и применён");
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
        reset_btn.connect_clicked(move |_| match reset_monitors_to_auto(&paths) {
            Ok(()) => {
                status_cb.ok("Конфиг мониторов сброшен на автоопределение");
                populate();
            }
            Err(e) => status_cb.error(&e),
        });
    }

    root
}

fn w_mode_at(options: &[String], idx: usize) -> String {
    options
        .get(idx)
        .cloned()
        .unwrap_or_else(|| "preferred".to_string())
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
