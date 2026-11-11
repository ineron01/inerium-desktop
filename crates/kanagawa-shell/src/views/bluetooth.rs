use std::cell::RefCell;
use std::process::Command;
use std::rc::Rc;
use std::thread;

use gtk4::prelude::*;
use gtk4::{Box, Button, Label, Orientation, ScrolledWindow, Switch};

use crate::hw;

#[derive(Clone)]
pub struct BtDevice {
    pub mac: String,
    pub name: String,
    pub connected: bool,
}

pub enum BtEvent {
    ScanDone(Vec<BtDevice>),
    ConnectDone(Vec<BtDevice>),
}

type BtCallback = Rc<dyn Fn(BtEvent)>;

thread_local! {
    static BT_CALLBACK: RefCell<Option<BtCallback>> = RefCell::new(None);
}

pub fn query_devices() -> Vec<BtDevice> {
    let mut devices = Vec::new();

    // Query connected devices first
    let mut connected_macs = Vec::new();
    if let Ok(out) = Command::new("bluetoothctl").args(["devices", "Connected"]).output() {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 && parts[0] == "Device" {
                connected_macs.push(parts[1].to_string());
            }
        }
    }

    if let Ok(out) = Command::new("bluetoothctl").args(["devices"]).output() {
        let text = String::from_utf8_lossy(&out.stdout);
        let mut seen = std::collections::HashSet::new();

        for line in text.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 && parts[0] == "Device" {
                let mac = parts[1].to_string();
                if seen.contains(&mac) {
                    continue;
                }
                seen.insert(mac.clone());
                let name = parts[2..].join(" ");
                let connected = connected_macs.contains(&mac);
                devices.push(BtDevice { mac, name, connected });
            }
        }
    }

    // Sort connected devices to the top, then alphabetically
    devices.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    devices
}

fn populate_bt_list(list_box: &Box, devices: Vec<BtDevice>) {
    if devices.is_empty() {
        let empty_lbl = Label::new(Some(if hw::is_bluetooth_enabled() {
            "Устройства не найдены. Нажмите «Поиск»"
        } else {
            "Bluetooth выключен"
        }));
        empty_lbl.add_css_class("cc-meta");
        empty_lbl.set_margin_top(20);
        list_box.append(&empty_lbl);
        return;
    }

    for dev in devices {
        let row = Button::new();
        row.add_css_class("card-item");
        if dev.connected {
            row.add_css_class("active");
        }

        let inner = Box::new(Orientation::Horizontal, 8);
        let ico_str = if dev.name.to_lowercase().contains("buds") || dev.name.to_lowercase().contains("head") {
            "󰋋"
        } else if dev.name.to_lowercase().contains("mouse") {
            "󰍽"
        } else if dev.name.to_lowercase().contains("key") {
            "󰌌"
        } else {
            "󰂯"
        };
        let ico = Label::new(Some(ico_str));
        ico.add_css_class("card-icon");
        inner.append(&ico);

        let txt = Box::new(Orientation::Vertical, 1);
        txt.set_hexpand(true);
        let name_lbl = Label::new(Some(&dev.name));
        name_lbl.add_css_class("card-title");
        name_lbl.set_xalign(0.0);
        name_lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        txt.append(&name_lbl);

        let mac_lbl = Label::new(Some(&dev.mac));
        mac_lbl.add_css_class("card-sub");
        mac_lbl.set_xalign(0.0);
        txt.append(&mac_lbl);
        inner.append(&txt);

        let badge = Label::new(Some(if dev.connected { "󰄬 Подключено" } else { "" }));
        badge.add_css_class("card-badge");
        badge.set_visible(dev.connected);
        inner.append(&badge);

        row.set_child(Some(&inner));

        let mac_target = dev.mac.clone();
        let is_conn = dev.connected;
        let r_ui = row.clone();
        let b_ui = badge.clone();

        row.connect_clicked(move |_| {
            r_ui.set_sensitive(false);
            b_ui.set_label(if is_conn { "󰑐 Отключение..." } else { "󰑐 Подключение..." });
            b_ui.set_visible(true);

            let target_mac = mac_target.clone();

            thread::spawn(move || {
                if is_conn {
                    let _ = Command::new("bluetoothctl").args(["disconnect", &target_mac]).output();
                } else {
                    let _ = Command::new("bluetoothctl").args(["connect", &target_mac]).output();
                }
                let fresh = query_devices();
                glib::MainContext::default().invoke(move || {
                    BT_CALLBACK.with(|cb| {
                        if let Some(ref f) = *cb.borrow() {
                            f(BtEvent::ConnectDone(fresh));
                        }
                    });
                });
            });
        });

        list_box.append(&row);
    }
}

pub fn build_bluetooth_view<FNav, FClose>(
    nav_cb: FNav,
    _close_cb: FClose,
) -> (Box, i32, i32)
where
    FNav: Fn(&str) + 'static + Clone,
    FClose: Fn() + 'static + Clone,
{
    let root = Box::new(Orientation::Vertical, 10);
    root.add_css_class("popup-container");
    root.set_size_request(380, -1);

    // Header
    let head = Box::new(Orientation::Horizontal, 8);
    let back_btn = Button::with_label("‹ Назад");
    back_btn.add_css_class("back-btn");
    let n_cb = nav_cb.clone();
    back_btn.connect_clicked(move |_| n_cb("control"));
    head.append(&back_btn);

    let title = Label::new(Some("Bluetooth"));
    title.add_css_class("cc-host");
    title.set_hexpand(true);
    title.set_xalign(0.0);
    head.append(&title);

    let pwr_switch = Switch::new();
    pwr_switch.set_active(hw::is_bluetooth_enabled());
    head.append(&pwr_switch);
    root.append(&head);

    // Subheader with Scan
    let subhead = Box::new(Orientation::Horizontal, 8);
    let lbl = Label::new(Some("УСТРОЙСТВА"));
    lbl.add_css_class("cc-label");
    lbl.set_xalign(0.0);
    lbl.set_hexpand(true);
    subhead.append(&lbl);

    let scan_btn = Button::with_label("󰑐 Поиск");
    scan_btn.add_css_class("cc-chip");
    subhead.append(&scan_btn);
    root.append(&subhead);

    // Device List
    let scroll = ScrolledWindow::new();
    scroll.add_css_class("list-scroll");
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroll.set_propagate_natural_height(true);
    scroll.set_min_content_height(160);
    scroll.set_max_content_height(300);
    scroll.set_vexpand(true);

    let list_box = Box::new(Orientation::Vertical, 4);

    let initial_devices = query_devices();
    populate_bt_list(&list_box, initial_devices);

    // Register callback on main thread
    let s_btn_ui = scan_btn.clone();
    let l_b_ui = list_box.clone();

    let cb = Rc::new(move |evt: BtEvent| {
        match evt {
            BtEvent::ScanDone(devices) => {
                s_btn_ui.set_label("󰑐 Поиск");
                s_btn_ui.set_sensitive(true);

                while let Some(child) = l_b_ui.first_child() {
                    l_b_ui.remove(&child);
                }
                populate_bt_list(&l_b_ui, devices);
            }
            BtEvent::ConnectDone(devices) => {
                while let Some(child) = l_b_ui.first_child() {
                    l_b_ui.remove(&child);
                }
                populate_bt_list(&l_b_ui, devices);
            }
        }
    });

    BT_CALLBACK.with(|c| {
        *c.borrow_mut() = Some(cb);
    });

    // Scan button logic with clear visual progress
    let s_btn_c = scan_btn.clone();
    scan_btn.connect_clicked(move |_| {
        s_btn_c.set_sensitive(false);
        s_btn_c.set_label("󰑐 Поиск устройств...");

        thread::spawn(move || {
            let _ = Command::new("bluetoothctl").args(["--timeout", "4", "scan", "on"]).output();
            let fresh = query_devices();
            glib::MainContext::default().invoke(move || {
                BT_CALLBACK.with(|cb| {
                    if let Some(ref f) = *cb.borrow() {
                        f(BtEvent::ScanDone(fresh));
                    }
                });
            });
        });
    });

    // Power switch toggling
    pwr_switch.connect_state_set(move |_, state| {
        hw::toggle_bluetooth(state);

        glib::timeout_add_local_once(std::time::Duration::from_millis(500), move || {
            let devs = if state { query_devices() } else { Vec::new() };
            BT_CALLBACK.with(|cb| {
                if let Some(ref f) = *cb.borrow() {
                    f(BtEvent::ScanDone(devs));
                }
            });
        });

        glib::Propagation::Proceed
    });

    scroll.set_child(Some(&list_box));
    root.append(&scroll);

    (root, 380, 340)
}
