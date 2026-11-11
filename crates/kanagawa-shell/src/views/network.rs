use std::cell::RefCell;
use std::collections::HashMap;
use std::process::Command;
use std::rc::Rc;
use std::thread;

use gtk4::prelude::*;
use gtk4::{Box, Button, Entry, Label, Orientation, ScrolledWindow, Switch};

use crate::hw;

#[derive(Clone)]
pub struct WifiAp {
    pub ssid: String,
    pub signal: u32,
    pub security: String,
    pub is_active: bool,
    pub is_saved: bool,
}

#[derive(Clone)]
pub struct SavedConn {
    pub uuid: String,
    pub _name: String,
}

#[allow(clippy::enum_variant_names)]
pub enum NetEvent {
    RescanDone(Vec<WifiAp>),
    ConnectDone {
        ssid: String,
        success: bool,
        error: Option<String>,
        networks: Vec<WifiAp>,
    },
    DisconnectDone {
        networks: Vec<WifiAp>,
    },
}

type NetCallback = Rc<dyn Fn(NetEvent)>;

thread_local! {
    static NET_CALLBACK: RefCell<Option<NetCallback>> = RefCell::new(None);
}

pub fn query_saved_wifi_connections() -> HashMap<String, SavedConn> {
    let mut saved = HashMap::new();
    if let Ok(out) = Command::new("nmcli")
        .args(["-t", "-f", "UUID,TYPE,NAME", "connection", "show"])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 3 {
                let uuid = parts[0].trim().to_string();
                let ctype = parts[1].trim();
                let name = parts[2].replace("\\:", ":").trim().to_string();
                if ctype == "802-11-wireless" || ctype == "wifi" {
                    let conn = SavedConn {
                        uuid: uuid.clone(),
                        _name: name.clone(),
                    };
                    saved.insert(name, conn);
                }
            }
        }
    }
    saved
}

pub fn query_wifi_networks() -> Vec<WifiAp> {
    if hw::is_hotspot_active() {
        return Vec::new();
    }
    let saved_map = query_saved_wifi_connections();
    let active_ssid = hw::get_active_wifi_ssid();
    let mut networks: Vec<WifiAp> = Vec::new();

    if let Ok(out) = Command::new("nmcli")
        .args(["-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "dev", "wifi", "list"])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);

        for line in text.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 4 {
                let in_use = parts[0] == "*";
                let ssid = parts[1].replace("\\:", ":").trim().to_string();
                if ssid.is_empty() {
                    continue;
                }
                let signal: u32 = parts[2].parse().unwrap_or(50);
                let security = parts[3].to_string();
                let is_saved = saved_map.contains_key(&ssid);
                let is_active = in_use || active_ssid.as_deref() == Some(&ssid);

                if let Some(existing) = networks.iter_mut().find(|n| n.ssid == ssid) {
                    existing.signal = existing.signal.max(signal);
                    if is_active {
                        existing.is_active = true;
                    }
                } else {
                    networks.push(WifiAp {
                        ssid,
                        signal,
                        security,
                        is_active,
                        is_saved,
                    });
                }
            }
        }
    }

    // Если сейчас подключены к сети, но она не попала в скан (скрытая сеть / лаг сканера)
    if let Some(ref act) = active_ssid {
        if let Some(existing) = networks.iter_mut().find(|n| &n.ssid == act) {
            existing.is_active = true;
        } else {
            networks.push(WifiAp {
                ssid: act.clone(),
                signal: 100,
                security: String::new(),
                is_active: true,
                is_saved: saved_map.contains_key(act),
            });
        }
    }

    // Sort: 1) Active first; 2) Saved networks by signal; 3) Unsaved networks by signal
    networks.sort_by(|a, b| {
        b.is_active
            .cmp(&a.is_active)
            .then_with(|| b.is_saved.cmp(&a.is_saved))
            .then_with(|| b.signal.cmp(&a.signal))
    });

    networks
}

fn populate_wifi_list(
    list_box: &Box,
    networks: Vec<WifiAp>,
    auth_box: &Box,
    auth_target_lbl: &Label,
    pass_entry: &Entry,
    error_lbl: &Label,
    current_target_ssid: &Rc<RefCell<String>>,
) {
    if hw::is_hotspot_active() {
        let card = Box::new(Orientation::Vertical, 10);
        card.add_css_class("cc-card");
        card.set_margin_top(8);
        card.set_margin_bottom(8);

        let top_row = Box::new(Orientation::Horizontal, 8);
        let ico = Label::new(Some("󱜠"));
        ico.add_css_class("card-icon");
        top_row.append(&ico);

        let title_box = Box::new(Orientation::Vertical, 2);
        title_box.set_hexpand(true);
        let title = Label::new(Some("Занято точкой доступа"));
        title.add_css_class("card-title");
        title.set_xalign(0.0);
        title_box.append(&title);

        let sub = Label::new(Some("Раздача Wi-Fi (точка доступа)"));
        sub.add_css_class("card-sub");
        sub.set_xalign(0.0);
        title_box.append(&sub);
        top_row.append(&title_box);
        card.append(&top_row);

        let desc = Label::new(Some(
            "Беспроводной модуль сейчас работает в режиме раздачи интернета (AP).\n\nДля сканирования и подключения к другим Wi-Fi сетям сначала выключите точку доступа."
        ));
        desc.add_css_class("cc-meta");
        desc.set_wrap(true);
        desc.set_xalign(0.0);
        card.append(&desc);

        let stop_btn = Button::with_label("󱜡  Выключить точку доступа");
        stop_btn.add_css_class("cc-chip");
        stop_btn.add_css_class("on");
        stop_btn.connect_clicked(|b| {
            b.set_sensitive(false);
            b.set_label("󰑐 Выключение...");
            thread::spawn(|| {
                let bin = hw::home_dir().join(".local/bin/kanagawa-hotspot");
                let mut cmd = if bin.exists() {
                    Command::new(bin)
                } else {
                    Command::new("kanagawa-hotspot")
                };
                let _ = cmd.arg("toggle").output();
                thread::sleep(std::time::Duration::from_millis(800));
                let nets = query_wifi_networks();
                glib::MainContext::default().invoke(move || {
                    NET_CALLBACK.with(|cb| {
                        if let Some(ref f) = *cb.borrow() {
                            f(NetEvent::RescanDone(nets));
                        }
                    });
                });
            });
        });
        card.append(&stop_btn);

        list_box.append(&card);
        return;
    }

    if networks.is_empty() {
        let empty_lbl = Label::new(Some(if hw::is_wifi_enabled() {
            "Сети не найдены. Нажмите «Обновить»"
        } else {
            "Wi-Fi выключен"
        }));
        empty_lbl.add_css_class("cc-meta");
        empty_lbl.set_margin_top(20);
        list_box.append(&empty_lbl);
        return;
    }

    for net in networks {
        let row = Button::new();
        row.add_css_class("card-item");
        if net.is_active {
            row.add_css_class("active");
        }

        let inner = Box::new(Orientation::Horizontal, 8);
        let sig_ico = if net.signal > 75 {
            "󰤨"
        } else if net.signal > 50 {
            "󰤥"
        } else if net.signal > 25 {
            "󰤢"
        } else {
            "󰤟"
        };
        let ico = Label::new(Some(sig_ico));
        ico.add_css_class("card-icon");
        inner.append(&ico);

        let txt = Box::new(Orientation::Vertical, 1);
        txt.set_hexpand(true);
        let name_lbl = Label::new(Some(&net.ssid));
        name_lbl.add_css_class("card-title");
        name_lbl.set_xalign(0.0);
        name_lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        txt.append(&name_lbl);

        let sec_str = if net.is_active {
            format!("Подключено • {}", if net.security.is_empty() { "Открытая сеть" } else { &net.security })
        } else if net.security.is_empty() {
            "Открытая сеть".to_string()
        } else {
            net.security.clone()
        };
        let sec_lbl = Label::new(Some(&sec_str));
        sec_lbl.add_css_class("card-sub");
        sec_lbl.set_xalign(0.0);
        txt.append(&sec_lbl);
        inner.append(&txt);

        // Badge: Active ("󰄬 В сети") or Saved ("󰄬 Сохранено")
        let badge = Label::new(None);
        badge.add_css_class("card-badge");
        if net.is_active {
            badge.set_label("󰄬 В сети");
            badge.add_css_class("active");
            badge.set_visible(true);
        } else if net.is_saved {
            badge.set_label("󰄬 Сохранено");
            badge.add_css_class("saved");
            badge.set_visible(true);
        } else {
            badge.set_visible(false);
        }
        inner.append(&badge);

        row.set_child(Some(&inner));

        let ssid_str = net.ssid.clone();
        let is_sec = !net.security.is_empty() && !net.security.contains("--");
        let a_box = auth_box.clone();
        let a_tgt = auth_target_lbl.clone();
        let p_ent = pass_entry.clone();
        let err_lbl_c = error_lbl.clone();
        let tgt_ref = current_target_ssid.clone();
        let r_ui = row.clone();
        let b_ui = badge.clone();
        let is_active = net.is_active;
        let is_saved = net.is_saved;

        row.connect_clicked(move |_| {
            if is_active {
                // Currently connected -> Disconnect
                let s = ssid_str.clone();
                b_ui.set_label("󰑐 Отключение...");
                b_ui.set_visible(true);
                r_ui.set_sensitive(false);

                thread::spawn(move || {
                    let saved_map = query_saved_wifi_connections();
                    if let Some(conn) = saved_map.get(&s) {
                        let _ = Command::new("nmcli").args(["connection", "down", "uuid", &conn.uuid]).output();
                    } else {
                        let _ = Command::new("nmcli").args(["connection", "down", "id", &s]).output();
                    }
                    let nets = query_wifi_networks();
                    glib::MainContext::default().invoke(move || {
                        NET_CALLBACK.with(|cb| {
                            if let Some(ref f) = *cb.borrow() {
                                f(NetEvent::DisconnectDone { networks: nets });
                            }
                        });
                    });
                });
            } else if is_saved {
                // Saved network -> Connect directly without asking for password!
                let s = ssid_str.clone();
                b_ui.set_label("󰑐 Подключение...");
                b_ui.set_visible(true);
                r_ui.set_sensitive(false);

                thread::spawn(move || {
                    let saved_map = query_saved_wifi_connections();
                    let res = if let Some(conn) = saved_map.get(&s) {
                        Command::new("nmcli").args(["connection", "up", "uuid", &conn.uuid]).output()
                    } else {
                        Command::new("nmcli").args(["connection", "up", "id", &s]).output()
                    };

                    let (success, err_msg) = match res {
                        Ok(r) => {
                            if r.status.success() {
                                (true, String::new())
                            } else {
                                let err = String::from_utf8_lossy(&r.stderr).trim().to_string();
                                let msg = if err.is_empty() {
                                    String::from_utf8_lossy(&r.stdout).trim().to_string()
                                } else {
                                    err
                                };
                                (false, msg)
                            }
                        }
                        Err(e) => (false, e.to_string()),
                    };
                    let nets = query_wifi_networks();
                    glib::MainContext::default().invoke(move || {
                        NET_CALLBACK.with(|cb| {
                            if let Some(ref f) = *cb.borrow() {
                                f(NetEvent::ConnectDone {
                                    ssid: s,
                                    success,
                                    error: if !success { Some(err_msg) } else { None },
                                    networks: nets,
                                });
                            }
                        });
                    });
                });
            } else if is_sec {
                // Unsaved secured network -> Open password input dialog
                *tgt_ref.borrow_mut() = ssid_str.clone();
                a_tgt.set_label(&format!("Подключение к «{}»", ssid_str));
                p_ent.set_text("");
                err_lbl_c.set_visible(false);
                err_lbl_c.set_text("");
                a_box.set_visible(true);
                p_ent.grab_focus();
            } else {
                // Open unsaved network -> Connect directly
                let s = ssid_str.clone();
                b_ui.set_label("󰑐 Подключение...");
                b_ui.set_visible(true);
                r_ui.set_sensitive(false);

                thread::spawn(move || {
                    let res = Command::new("nmcli").args(["dev", "wifi", "connect", &s]).output();
                    let (success, err_msg) = match res {
                        Ok(r) => {
                            if r.status.success() {
                                (true, String::new())
                            } else {
                                let err = String::from_utf8_lossy(&r.stderr).trim().to_string();
                                let msg = if err.is_empty() {
                                    String::from_utf8_lossy(&r.stdout).trim().to_string()
                                } else {
                                    err
                                };
                                (false, msg)
                            }
                        }
                        Err(e) => (false, e.to_string()),
                    };
                    let nets = query_wifi_networks();
                    glib::MainContext::default().invoke(move || {
                        NET_CALLBACK.with(|cb| {
                            if let Some(ref f) = *cb.borrow() {
                                f(NetEvent::ConnectDone {
                                    ssid: s,
                                    success,
                                    error: if !success { Some(err_msg) } else { None },
                                    networks: nets,
                                });
                            }
                        });
                    });
                });
            }
        });

        list_box.append(&row);
    }
}

pub fn build_network_view<FNav, FClose>(
    nav_cb: FNav,
    _close_cb: FClose,
) -> (Box, i32, i32)
where
    FNav: Fn(&str) + 'static + Clone,
    FClose: Fn() + 'static + Clone,
{
    let root = Box::new(Orientation::Vertical, 10);
    root.add_css_class("popup-container");
    root.set_size_request(390, -1);

    // Header
    let head = Box::new(Orientation::Horizontal, 8);
    let back_btn = Button::with_label("‹ Назад");
    back_btn.add_css_class("back-btn");
    let n_cb = nav_cb.clone();
    back_btn.connect_clicked(move |_| n_cb("control"));
    head.append(&back_btn);

    let title_box = Box::new(Orientation::Vertical, 1);
    title_box.set_hexpand(true);

    let title = Label::new(Some("Wi-Fi и сети"));
    title.add_css_class("cc-host");
    title.set_xalign(0.0);
    title_box.append(&title);

    let status_sub = Label::new(None);
    status_sub.add_css_class("card-sub");
    status_sub.set_xalign(0.0);
    title_box.append(&status_sub);

    head.append(&title_box);

    let pwr_switch = Switch::new();
    pwr_switch.set_active(hw::is_wifi_enabled());
    head.append(&pwr_switch);
    root.append(&head);

    // Subheader with Rescan
    let subhead = Box::new(Orientation::Horizontal, 8);
    let lbl = Label::new(Some("ДОСТУПНЫЕ СЕТИ"));
    lbl.add_css_class("cc-label");
    lbl.set_xalign(0.0);
    lbl.set_hexpand(true);
    subhead.append(&lbl);

    let rescan_btn = Button::with_label("󰑐 Обновить");
    rescan_btn.add_css_class("cc-chip");
    subhead.append(&rescan_btn);

    let lbl_c = lbl.clone();
    let rescan_c = rescan_btn.clone();
    let update_status_sub = {
        let sub = status_sub.clone();
        Rc::new(move || {
            if hw::is_hotspot_active() {
                sub.set_label("󱜠 Раздача Wi-Fi активна");
                lbl_c.set_label("ТОЧКА ДОСТУПА");
                rescan_c.set_visible(false);
            } else if !hw::is_wifi_enabled() {
                sub.set_label("Беспроводная сеть выключена");
                lbl_c.set_label("ДОСТУПНЫЕ СЕТИ");
                rescan_c.set_visible(true);
            } else if let Some(ssid) = hw::get_active_wifi_ssid() {
                sub.set_label(&format!("󰄬 Подключено: {}", ssid));
                lbl_c.set_label("ДОСТУПНЫЕ СЕТИ");
                rescan_c.set_visible(true);
            } else {
                sub.set_label("Не подключено");
                lbl_c.set_label("ДОСТУПНЫЕ СЕТИ");
                rescan_c.set_visible(true);
            }
        })
    };
    update_status_sub();
    root.append(&subhead);

    // Inline Auth Box (hidden by default)
    let auth_box = Box::new(Orientation::Vertical, 6);
    auth_box.add_css_class("cc-card");
    auth_box.set_visible(false);

    let auth_target_lbl = Label::new(None);
    auth_target_lbl.add_css_class("card-title");
    auth_target_lbl.set_xalign(0.0);
    auth_box.append(&auth_target_lbl);

    let pass_entry = Entry::new();
    pass_entry.set_placeholder_text(Some("Пароль Wi-Fi сети"));
    pass_entry.set_visibility(false);
    auth_box.append(&pass_entry);

    let error_lbl = Label::new(None);
    error_lbl.add_css_class("auth-error");
    error_lbl.set_xalign(0.0);
    error_lbl.set_wrap(true);
    error_lbl.set_visible(false);
    auth_box.append(&error_lbl);

    let auth_actions = Box::new(Orientation::Horizontal, 8);
    let connect_btn = Button::with_label("Подключить");
    connect_btn.add_css_class("cc-chip");
    connect_btn.add_css_class("on");

    let cancel_btn = Button::with_label("Отмена");
    cancel_btn.add_css_class("cc-chip");

    let a_box_c = auth_box.clone();
    let err_lbl_clear = error_lbl.clone();
    cancel_btn.connect_clicked(move |_| {
        err_lbl_clear.set_visible(false);
        err_lbl_clear.set_text("");
        a_box_c.set_visible(false);
    });

    auth_actions.append(&connect_btn);
    auth_actions.append(&cancel_btn);
    auth_box.append(&auth_actions);
    root.append(&auth_box);

    // Enter in pass_entry triggers connect_btn
    let conn_click_trigger = connect_btn.clone();
    pass_entry.connect_activate(move |_| {
        conn_click_trigger.emit_clicked();
    });

    // Networks List
    let scroll = ScrolledWindow::new();
    scroll.add_css_class("list-scroll");
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroll.set_propagate_natural_height(true);
    scroll.set_min_content_height(180);
    scroll.set_max_content_height(320);
    scroll.set_vexpand(true);

    let list_box = Box::new(Orientation::Vertical, 4);
    let current_target_ssid = Rc::new(RefCell::new(String::new()));

    // Initial population
    let initial_networks = query_wifi_networks();
    populate_wifi_list(
        &list_box,
        initial_networks,
        &auth_box,
        &auth_target_lbl,
        &pass_entry,
        &error_lbl,
        &current_target_ssid,
    );

    // Register thread-local callback on main thread
    let r_btn_ui = rescan_btn.clone();
    let c_btn_ui = connect_btn.clone();
    let l_b_ui = list_box.clone();
    let a_b_ui = auth_box.clone();
    let a_tgt_ui = auth_target_lbl.clone();
    let p_val_ui = pass_entry.clone();
    let err_ui = error_lbl.clone();
    let c_tgt_ui = current_target_ssid.clone();

    let u_sub = update_status_sub.clone();
    let cb = Rc::new(move |evt: NetEvent| {
        u_sub();
        match evt {
            NetEvent::RescanDone(networks) => {
                r_btn_ui.set_label("󰑐 Обновить");
                r_btn_ui.set_sensitive(true);

                while let Some(child) = l_b_ui.first_child() {
                    l_b_ui.remove(&child);
                }
                populate_wifi_list(
                    &l_b_ui,
                    networks,
                    &a_b_ui,
                    &a_tgt_ui,
                    &p_val_ui,
                    &err_ui,
                    &c_tgt_ui,
                );
            }
            NetEvent::ConnectDone { ssid, success, error, networks } => {
                c_btn_ui.set_sensitive(true);
                c_btn_ui.set_label("Подключить");

                if success {
                    a_b_ui.set_visible(false);
                    err_ui.set_visible(false);
                    err_ui.set_text("");
                } else {
                    let err_text = error.unwrap_or_else(|| "Ошибка подключения".to_string());
                    let clean = err_text.trim_start_matches("Error: ").trim_start_matches("Ошибка: ");
                    err_ui.set_text(&format!("󰅖 {}", clean));
                    err_ui.set_visible(true);
                    if a_b_ui.is_visible() {
                        a_tgt_ui.set_label(&format!("Подключение к «{}»", ssid));
                        p_val_ui.grab_focus();
                    }
                }

                while let Some(child) = l_b_ui.first_child() {
                    l_b_ui.remove(&child);
                }
                populate_wifi_list(
                    &l_b_ui,
                    networks,
                    &a_b_ui,
                    &a_tgt_ui,
                    &p_val_ui,
                    &err_ui,
                    &c_tgt_ui,
                );
            }
            NetEvent::DisconnectDone { networks } => {
                while let Some(child) = l_b_ui.first_child() {
                    l_b_ui.remove(&child);
                }
                populate_wifi_list(
                    &l_b_ui,
                    networks,
                    &a_b_ui,
                    &a_tgt_ui,
                    &p_val_ui,
                    &err_ui,
                    &c_tgt_ui,
                );
            }
        }
    });

    NET_CALLBACK.with(|c| {
        *c.borrow_mut() = Some(cb);
    });

    // Rescan button clicked
    let r_btn_c = rescan_btn.clone();
    rescan_btn.connect_clicked(move |_| {
        r_btn_c.set_sensitive(false);
        r_btn_c.set_label("󰑐 Поиск сетей...");

        thread::spawn(move || {
            let _ = Command::new("nmcli").args(["dev", "wifi", "rescan"]).output();
            let fresh = query_wifi_networks();
            glib::MainContext::default().invoke(move || {
                NET_CALLBACK.with(|cb| {
                    if let Some(ref f) = *cb.borrow() {
                        f(NetEvent::RescanDone(fresh));
                    }
                });
            });
        });
    });

    // Connect button clicked inside auth box
    let c_tgt = current_target_ssid.clone();
    let p_val = pass_entry.clone();
    let conn_btn_c = connect_btn.clone();
    let err_click_ref = error_lbl.clone();

    connect_btn.connect_clicked(move |_| {
        let ssid = c_tgt.borrow().clone();
        let password = p_val.text().to_string();
        if ssid.is_empty() {
            return;
        }

        conn_btn_c.set_sensitive(false);
        conn_btn_c.set_label("󰑐 Подключение...");
        err_click_ref.set_visible(false);
        err_click_ref.set_text("");

        thread::spawn(move || {
            let res = Command::new("nmcli")
                .args(["dev", "wifi", "connect", &ssid, "password", &password])
                .output();
            let (success, err_msg) = match res {
                Ok(r) => {
                    if r.status.success() {
                        (true, String::new())
                    } else {
                        let err = String::from_utf8_lossy(&r.stderr).trim().to_string();
                        let msg = if err.is_empty() {
                            String::from_utf8_lossy(&r.stdout).trim().to_string()
                        } else {
                            err
                        };
                        (false, msg)
                    }
                }
                Err(e) => (false, e.to_string()),
            };
            let fresh = query_wifi_networks();
            glib::MainContext::default().invoke(move || {
                NET_CALLBACK.with(|cb| {
                    if let Some(ref f) = *cb.borrow() {
                        f(NetEvent::ConnectDone {
                            ssid,
                            success,
                            error: if !success { Some(err_msg) } else { None },
                            networks: fresh,
                        });
                    }
                });
            });
        });
    });

    // Power switch toggling
    let u_sub_pwr = update_status_sub.clone();
    pwr_switch.connect_state_set(move |_, state| {
        hw::toggle_wifi(state);
        let u = u_sub_pwr.clone();

        glib::timeout_add_local_once(std::time::Duration::from_millis(500), move || {
            u();
            let nets = if state { query_wifi_networks() } else { Vec::new() };
            NET_CALLBACK.with(|cb| {
                if let Some(ref f) = *cb.borrow() {
                    f(NetEvent::RescanDone(nets));
                }
            });
        });

        glib::Propagation::Proceed
    });

    scroll.set_child(Some(&list_box));
    root.append(&scroll);

    (root, 390, 380)
}
