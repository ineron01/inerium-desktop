use std::cell::RefCell;
use std::process::Command;
use std::rc::Rc;
use std::thread;

use gtk4::prelude::*;
use gtk4::{Align, Box, Button, Grid, Label, Orientation, PolicyType, Scale, ScrolledWindow};

use crate::hw;
use inerium_i18n::{tr, tr_fmt, tr_plural};

fn make_label(text: &str, css_class: &str, xalign: f32) -> Label {
    let lbl = Label::new(Some(text));
    lbl.add_css_class(css_class);
    lbl.set_xalign(xalign);
    lbl
}

type GpuCallback = Rc<dyn Fn(bool, &'static str)>;
type DdcCallback = Rc<dyn Fn(u32)>;
type UpdatesCallback = Rc<dyn Fn(hw::SystemUpdates)>;

thread_local! {
    static GPU_CALLBACK: RefCell<Option<GpuCallback>> = RefCell::new(None);
    static DDC_CALLBACK: RefCell<Option<DdcCallback>> = RefCell::new(None);
    static UPDATES_CALLBACK: RefCell<Option<UpdatesCallback>> = RefCell::new(None);
}

pub fn on_ddc_updated(val: u32) {
    DDC_CALLBACK.with(|cb| {
        if let Some(ref f) = *cb.borrow() {
            f(val);
        }
    });
}

pub fn build_control_center<FNav, FClose>(
    nav_cb: FNav,
    close_cb: FClose,
) -> (Box, i32, i32)
where
    FNav: Fn(&str) + 'static + Clone,
    FClose: Fn() + 'static + Clone,
{
    let root = Box::new(Orientation::Vertical, 10);
    root.add_css_class("cc");
    root.set_size_request(384, -1);

    // ---------------------------------------------------------
    // 1. ШАПКА
    // ---------------------------------------------------------
    let head = Box::new(Orientation::Horizontal, 8);
    head.add_css_class("cc-head");

    let head_txt = Box::new(Orientation::Vertical, 2);
    head_txt.set_hexpand(true);
    head_txt.append(&make_label(&hw::get_hostname(), "cc-host", 0.0));

    let meta = Box::new(Orientation::Horizontal, 8);
    let bat_lbl = make_label(&hw::get_battery_line(), "cc-meta-accent", 0.0);
    let up_lbl = make_label(&hw::get_uptime_line(), "cc-meta", 0.0);
    meta.append(&bat_lbl);
    meta.append(&up_lbl);

    let upds = hw::get_updates_info();
    let upd_btn = Button::new();
    upd_btn.add_css_class("cc-meta-btn");
    upd_btn.set_valign(Align::Center);

    let upd_lbl = make_label("", "cc-meta-btn-lbl", 0.0);
    upd_btn.set_child(Some(&upd_lbl));

    fn apply_updates_ui(btn: &Button, lbl: &Label, info: &hw::SystemUpdates) {
        if info.total > 0 {
            lbl.set_label(&format!("󰚰 {}", info.total));
            btn.add_css_class("has-updates");

            let pkg_word = tr_plural(info.total, "package", "packages", "пакет", "пакета", "пакетов");
            let mut tt = tr_fmt!(
                "Ready to update: {total} {pkg_word}\n• Repositories: {repo}\n• AUR: {aur}",
                "Готово к обновлению: {total} {pkg_word}\n• Репозитории: {repo}\n• AUR: {aur}",
                total = info.total,
                pkg_word = pkg_word,
                repo = info.repo,
                aur = info.aur,
            );
            if !info.sample_packages.is_empty() {
                tt.push_str(tr("\n\nPackages:\n  ", "\n\nПакеты:\n  "));
                tt.push_str(&info.sample_packages.join(", "));
            }
            tt.push_str(tr(
                "\n\nLMB: Update system (paru)\nRMB: Recheck",
                "\n\nЛКМ: Обновить систему (paru)\nПКМ: Перепроверить",
            ));
            btn.set_tooltip_text(Some(&tt));
        } else {
            lbl.set_label("󰚰 0");
            btn.remove_css_class("has-updates");
            btn.set_tooltip_text(Some(tr(
                "All packages up to date\nLMB: Update (paru)\nRMB: Recheck",
                "Все пакеты обновлены\nЛКМ: Обновить (paru)\nПКМ: Перепроверить",
            )));
        }
    }

    apply_updates_ui(&upd_btn, &upd_lbl, &upds);

    {
        let u_btn = upd_btn.clone();
        let u_lbl = upd_lbl.clone();
        UPDATES_CALLBACK.with(|cb| {
            *cb.borrow_mut() = Some(Rc::new(move |fresh| {
                apply_updates_ui(&u_btn, &u_lbl, &fresh);
            }));
        });
    }

    let c_cb_upd = close_cb.clone();
    let upd_lbl_clone = upd_lbl.clone();

    let click_gesture = gtk4::GestureClick::new();
    click_gesture.set_button(0);
    click_gesture.connect_pressed(move |gesture, _, _, _| {
        let btn = gesture.current_button();
        if btn == 1 {
            c_cb_upd();
            thread::spawn(|| {
                let _ = Command::new("uwsm")
                    .args([
                        "app",
                        "--",
                        "kitty",
                        "-e",
                        "sh",
                        "-c",
                        "paru; rc=$?; inerium-updates --check >/dev/null 2>&1 || true; if [ \"$rc\" -ne 0 ]; then printf '\\n[paru завершился с кодом %s] Нажмите Enter для закрытия...' \"$rc\"; read -r _; fi",
                    ])
                    .spawn();
            });
        } else if btn == 3 {
            upd_lbl_clone.set_label("󰑐 ...");
            thread::spawn(move || {
                let _ = Command::new("inerium-updates").arg("--check").output();
                let fresh = hw::get_updates_info();
                glib::MainContext::default().invoke(move || {
                    UPDATES_CALLBACK.with(|cb| {
                        if let Some(ref f) = *cb.borrow() {
                            f(fresh);
                        }
                    });
                });
            });
        }
    });
    upd_btn.add_controller(click_gesture);
    meta.append(&upd_btn);
    head_txt.append(&meta);
    head.append(&head_txt);

    // Кнопка перехода в галерею обоев
    let wall_btn = Button::with_label("󰸉");
    wall_btn.add_css_class("cc-icon-btn");
    wall_btn.set_tooltip_text(Some(tr("Desktop Wallpapers", "Обои рабочего стола")));
    wall_btn.set_valign(Align::Center);
    let n_cb_w = nav_cb.clone();
    wall_btn.connect_clicked(move |_| n_cb_w("wallpaper"));
    head.append(&wall_btn);

    // Кнопка автоблокировки экрана (ингибитор)
    let idle_btn = Button::new();
    idle_btn.add_css_class("cc-icon-btn");
    idle_btn.set_valign(Align::Center);

    let is_idle_active = Rc::new(RefCell::new(hw::is_hypridle_active()));
    let update_idle_btn = {
        let idle_btn = idle_btn.clone();
        let is_idle_active = is_idle_active.clone();
        Rc::new(move || {
            let active = *is_idle_active.borrow();
            if active {
                idle_btn.set_label("󰒳");
                idle_btn.remove_css_class("amber");
                idle_btn.remove_css_class("on");
                idle_btn.set_tooltip_text(Some(tr("Autolock: On", "Автоблокировка: вкл")));
            } else {
                idle_btn.set_label("󰈈");
                idle_btn.add_css_class("amber");
                idle_btn.add_css_class("on");
                idle_btn.set_tooltip_text(Some(tr("Autolock: Off", "Автоблокировка: выкл")));
            }
        })
    };
    update_idle_btn();

    let i_flag = is_idle_active.clone();
    let u_idle = update_idle_btn.clone();
    idle_btn.connect_clicked(move |_| {
        let next = !*i_flag.borrow();
        *i_flag.borrow_mut() = next;
        u_idle(); // Instant UI update!
        hw::toggle_hypridle(next);
    });
    head.append(&idle_btn);

    // Кнопка блокировки
    let lock_btn = Button::with_label("󰌾");
    lock_btn.add_css_class("cc-icon-btn");
    lock_btn.set_tooltip_text(Some(tr("Lock", "Заблокировать")));
    lock_btn.set_valign(Align::Center);
    let c_cb = close_cb.clone();
    lock_btn.connect_clicked(move |_| {
        c_cb();
        thread::spawn(|| {
            let _ = Command::new("sh").args(["-c", "hyprlock || loginctl lock-session"]).output();
        });
    });
    head.append(&lock_btn);

    // Кнопка питания
    let power_btn = Button::with_label("⏻");
    power_btn.add_css_class("cc-icon-btn");
    power_btn.add_css_class("danger");
    power_btn.set_tooltip_text(Some(tr("Power menu", "Меню питания")));
    power_btn.set_valign(Align::Center);
    let n_cb = nav_cb.clone();
    power_btn.connect_clicked(move |_| {
        n_cb("power");
    });
    head.append(&power_btn);
    root.append(&head);

    let content = Box::new(Orientation::Vertical, 10);
    content.set_vexpand(true);
    content.set_hexpand(true);

    // ---------------------------------------------------------
    // 2. ПЛИТКИ БЫСТРЫХ ПЕРЕКЛЮЧАТЕЛЕЙ (3x2 Grid)
    // ---------------------------------------------------------
    let grid = Grid::new();
    grid.set_column_spacing(8);
    grid.set_row_spacing(8);
    grid.set_column_homogeneous(true);

    fn create_tile(
        icon: &str,
        title: &str,
        color_class: &str,
        has_more: bool,
    ) -> (Box, Button, Option<Button>, Label, Label) {
        let tile = Box::new(Orientation::Horizontal, 0);
        tile.add_css_class("cc-tile");
        if !color_class.is_empty() {
            tile.add_css_class(color_class);
        }

        let main = Button::new();
        main.add_css_class("cc-tile-main");
        main.set_hexpand(true);

        let inner = Box::new(Orientation::Horizontal, 9);
        let ico = make_label(icon, "cc-tile-ico", 0.5);
        inner.append(&ico);

        let txt = Box::new(Orientation::Vertical, 1);
        txt.set_valign(Align::Center);
        txt.append(&make_label(title, "cc-tile-title", 0.0));
        let sub = make_label("", "cc-tile-sub", 0.0);
        sub.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        sub.set_max_width_chars(14);
        txt.append(&sub);
        inner.append(&txt);

        main.set_child(Some(&inner));
        tile.append(&main);

        let more_btn = if has_more {
            let more = Button::with_label("›");
            more.add_css_class("cc-tile-more");
            more.set_tooltip_text(Some(tr("Details", "Подробнее")));
            tile.append(&more);
            Some(more)
        } else {
            None
        };

        (tile, main, more_btn, ico, sub)
    }

    // Состояния плит
    let is_wifi = Rc::new(RefCell::new(hw::is_wifi_enabled()));
    let is_bt = Rc::new(RefCell::new(hw::is_bluetooth_enabled()));
    let is_dnd = Rc::new(RefCell::new(hw::is_dnd_active()));
    let is_mic_muted = Rc::new(RefCell::new(hw::is_mic_muted()));

    // 4 основных плитки (2x2 Grid)
    let (wifi_tile, wifi_btn, wifi_more, _wifi_ico, wifi_sub) = create_tile("󰤨", "Wi-Fi", "", true);
    let (bt_tile, bt_btn, bt_more, _bt_ico, bt_sub) = create_tile("󰂯", "Bluetooth", "blue", true);
    let (dnd_tile, dnd_btn, _dnd_more, _dnd_ico, dnd_sub) = create_tile("󰂛", tr("Do Not Disturb", "Не беспокоить"), "amber", false);
    let (mic_tile, mic_btn, _mic_more, mic_ico, mic_sub) = create_tile("󰍬", tr("Microphone", "Микрофон"), "red", false);

    // Первоначальная и динамическая отрисовка плиток
    let update_tiles = {
        let wifi_tile = wifi_tile.clone();
        let wifi_sub = wifi_sub.clone();
        let bt_tile = bt_tile.clone();
        let bt_sub = bt_sub.clone();
        let dnd_tile = dnd_tile.clone();
        let dnd_sub = dnd_sub.clone();
        let mic_tile = mic_tile.clone();
        let mic_sub = mic_sub.clone();
        let mic_ico = mic_ico.clone();
        let is_wifi = is_wifi.clone();
        let is_bt = is_bt.clone();
        let is_dnd = is_dnd.clone();
        let is_mic_muted = is_mic_muted.clone();

        Rc::new(move || {
            let wf = *is_wifi.borrow();
            if wf {
                wifi_tile.add_css_class("on");
                if hw::is_hotspot_active() {
                    wifi_sub.set_label(tr("Hotspot", "Точка доступа"));
                } else if let Some(ref ssid) = hw::get_active_wifi_ssid() {
                    wifi_sub.set_label(ssid);
                } else {
                    wifi_sub.set_label(tr("Connected", "Включен"));
                }
            } else {
                wifi_tile.remove_css_class("on");
                wifi_sub.set_label(tr("Disabled", "Выключен"));
            }

            let bt = *is_bt.borrow();
            if bt {
                bt_tile.add_css_class("on");
                bt_sub.set_label(tr("Active", "Активен"));
            } else {
                bt_tile.remove_css_class("on");
                bt_sub.set_label(tr("Disabled", "Выключен"));
            }

            let dnd = *is_dnd.borrow();
            if dnd {
                dnd_tile.add_css_class("on");
                dnd_sub.set_label(tr("Notifications hidden", "Уведомления скрыты"));
            } else {
                dnd_tile.remove_css_class("on");
                dnd_sub.set_label(tr("Off", "Выключено"));
            }

            let mm = *is_mic_muted.borrow();
            if mm {
                mic_tile.add_css_class("on");
                mic_ico.set_label("󰍭");
                mic_sub.set_label(tr("Muted", "Выключен"));
            } else {
                mic_tile.remove_css_class("on");
                mic_ico.set_label("󰍬");
                mic_sub.set_label(tr("Active", "Включен"));
            }
        })
    };
    update_tiles();

    // Клики по основным плиткам с мгновенным обновлением UI
    let u_wf = update_tiles.clone();
    let w_flag = is_wifi.clone();
    wifi_btn.connect_clicked(move |_| {
        let next = !*w_flag.borrow();
        *w_flag.borrow_mut() = next;
        u_wf(); // Мгновенный отклик в интерфейсе
        hw::toggle_wifi(next);
    });
    if let Some(more) = wifi_more {
        let n = nav_cb.clone();
        more.connect_clicked(move |_| n("network"));
    }

    let u_bt = update_tiles.clone();
    let b_flag = is_bt.clone();
    bt_btn.connect_clicked(move |_| {
        let next = !*b_flag.borrow();
        *b_flag.borrow_mut() = next;
        u_bt(); // Мгновенный отклик в интерфейсе
        hw::toggle_bluetooth(next);
    });
    if let Some(more) = bt_more {
        let n = nav_cb.clone();
        more.connect_clicked(move |_| n("bluetooth"));
    }

    let u_dnd = update_tiles.clone();
    let d_flag = is_dnd.clone();
    dnd_btn.connect_clicked(move |_| {
        let next = !*d_flag.borrow();
        *d_flag.borrow_mut() = next;
        u_dnd(); // Мгновенный отклик в интерфейсе
        hw::toggle_dnd();
    });

    let u_mic = update_tiles.clone();
    let m_flag = is_mic_muted.clone();
    mic_btn.connect_clicked(move |_| {
        let next = !*m_flag.borrow();
        *m_flag.borrow_mut() = next;
        u_mic(); // Мгновенный отклик в интерфейсе
        hw::toggle_mic_mute();
    });

    grid.attach(&wifi_tile, 0, 0, 1, 1);
    grid.attach(&bt_tile, 1, 0, 1, 1);
    grid.attach(&dnd_tile, 0, 1, 1, 1);
    grid.attach(&mic_tile, 1, 1, 1, 1);
    content.append(&grid);

    // ---------------------------------------------------------
    // 3. СЛАЙДЕРЫ (Громкость, Ноутбук, Внешний экран)
    // ---------------------------------------------------------
    let sliders = Box::new(Orientation::Vertical, 4);
    sliders.add_css_class("cc-card");

    // Слайдер громкости
    let vol_row = Box::new(Orientation::Horizontal, 6);
    let vol_ico = Button::with_label(if hw::is_volume_muted() { "󰝟" } else { "󰕾" });
    vol_ico.add_css_class("cc-slider-ico");
    vol_ico.set_tooltip_text(Some("Mute"));
    let v_ico_clone = vol_ico.clone();
    vol_ico.connect_clicked(move |_| {
        hw::toggle_volume_mute();
        let muted = hw::is_volume_muted();
        v_ico_clone.set_label(if muted { "󰝟" } else { "󰕾" });
    });
    vol_row.append(&vol_ico);

    let cur_vol = hw::get_volume();
    let vol_sc = Scale::with_range(Orientation::Horizontal, 0.0, 150.0, 1.0);
    vol_sc.add_css_class("cc-scale");
    vol_sc.set_hexpand(true);
    vol_sc.set_value(cur_vol as f64);
    vol_row.append(&vol_sc);

    let vol_pct = make_label(&format!("{}%", cur_vol), "cc-pct", 1.0);
    vol_row.append(&vol_pct);

    let vol_more = Button::with_label("›");
    vol_more.add_css_class("cc-slider-ico");
    vol_more.set_tooltip_text(Some(tr("Audio Devices", "Аудиоустройства")));
    let n_cb_vol = nav_cb.clone();
    vol_more.connect_clicked(move |_| n_cb_vol("audio"));
    vol_row.append(&vol_more);

    let v_pct_clone = vol_pct.clone();
    vol_sc.connect_value_changed(move |sc| {
        let val = sc.value() as u32;
        v_pct_clone.set_label(&format!("{}%", val));
        hw::set_volume(val);
    });
    sliders.append(&vol_row);

    // Слайдер яркости ноутбука (скрыт на ПК без /sys/class/backlight)
    if hw::has_backlight() {
        let br_row = Box::new(Orientation::Horizontal, 6);
        let br_ico = Button::with_label("󰃠");
        br_ico.add_css_class("cc-slider-ico");
        br_ico.set_tooltip_text(Some(tr("Laptop display brightness", "Яркость экрана ноутбука")));
        br_row.append(&br_ico);

        let cur_br = hw::get_laptop_brightness();
        let br_sc = Scale::with_range(Orientation::Horizontal, 5.0, 100.0, 1.0);
        br_sc.add_css_class("cc-scale");
        br_sc.set_hexpand(true);
        br_sc.set_value(cur_br as f64);
        br_row.append(&br_sc);

        let br_pct = make_label(&format!("{}%", cur_br), "cc-pct", 1.0);
        br_row.append(&br_pct);

        let b_pct_clone = br_pct.clone();
        br_sc.connect_value_changed(move |sc| {
            let val = sc.value() as u32;
            b_pct_clone.set_label(&format!("{}%", val));
            hw::set_laptop_brightness(val);
        });
        sliders.append(&br_row);
    }

    // Слайдер яркости внешнего монитора (DDC)
    if hw::has_external_monitor() {
        let ext_row = Box::new(Orientation::Horizontal, 6);
        let ext_ico = Button::with_label("󰍹");
        ext_ico.add_css_class("cc-slider-ico");
        ext_ico.set_tooltip_text(Some(tr("External monitor brightness (DDC)", "Яркость внешнего монитора (DDC)")));
        ext_row.append(&ext_ico);

        let cur_ext = hw::get_cached_ddc_brightness();
        let ext_sc = Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 1.0);
        ext_sc.add_css_class("cc-scale");
        ext_sc.add_css_class("amber");
        ext_sc.set_hexpand(true);
        ext_sc.set_value(cur_ext as f64);
        ext_row.append(&ext_sc);

        let ext_pct = make_label(&format!("{}%", cur_ext), "cc-pct", 1.0);
        ext_row.append(&ext_pct);

        let e_pct_clone = ext_pct.clone();
        ext_sc.connect_value_changed(move |sc| {
            let val = sc.value() as u32;
            e_pct_clone.set_label(&format!("{}%", val));
            hw::set_ddc_brightness_async(val);
        });

        // Асинхронное освежение точного аппаратного значения
        let e_sc_refresh = ext_sc.clone();
        let e_pct_refresh = ext_pct.clone();
        DDC_CALLBACK.with(|cb| {
            *cb.borrow_mut() = Some(Rc::new(move |v: u32| {
                e_sc_refresh.set_value(v as f64);
                e_pct_refresh.set_label(&format!("{}%", v));
            }));
        });
        hw::refresh_ddc_brightness_async();

        sliders.append(&ext_row);
    }
    content.append(&sliders);

    // ---------------------------------------------------------
    // 4. ПИТАНИЕ И ПРОФИЛИ
    // ---------------------------------------------------------
    let has_ppd = hw::has_power_profiles();
    let has_fan_telem = hw::has_fan_telemetry();
    let has_ec = hw::has_msi_ec();
    let has_bat_thresh = hw::has_battery_threshold();

    if has_ppd || has_fan_telem || has_bat_thresh {
        let pw_title = if has_fan_telem {
            tr("POWER & COOLING", "ПИТАНИЕ И ОХЛАЖДЕНИЕ")
        } else {
            tr("POWER", "ПИТАНИЕ")
        };
        let pw_lbl = make_label(pw_title, "cc-label", 0.0);
        content.append(&pw_lbl);
    }

    if has_ppd {
        let seg = Box::new(Orientation::Horizontal, 2);
        seg.add_css_class("cc-seg");
        seg.set_homogeneous(true);

        let eco_btn = Button::with_label(&format!("󰌪 {}", tr("Eco", "Эконом")));
        eco_btn.add_css_class("eco");
        let bal_btn = Button::with_label(&format!("󰾅 {}", tr("Balance", "Баланс")));
        let turbo_btn = Button::with_label(&format!("󰓅 {}", tr("Turbo", "Турбо")));
        turbo_btn.add_css_class("turbo");

        let cur_prof = hw::get_power_profile();
        match cur_prof.as_str() {
            "power-saver" => eco_btn.add_css_class("active"),
            "performance" => turbo_btn.add_css_class("active"),
            _ => bal_btn.add_css_class("active"),
        }

        fn select_profile(target: &str, e: &Button, b: &Button, t: &Button) {
            e.remove_css_class("active");
            b.remove_css_class("active");
            t.remove_css_class("active");
            match target {
                "power-saver" => e.add_css_class("active"),
                "performance" => t.add_css_class("active"),
                _ => b.add_css_class("active"),
            }
            hw::set_power_profile(target);
        }

        let e_c = eco_btn.clone();
        let b_c = bal_btn.clone();
        let t_c = turbo_btn.clone();
        eco_btn.connect_clicked({
            let e = e_c.clone();
            let b = b_c.clone();
            let t = t_c.clone();
            move |_| select_profile("power-saver", &e, &b, &t)
        });
        bal_btn.connect_clicked({
            let e = e_c.clone();
            let b = b_c.clone();
            let t = t_c.clone();
            move |_| select_profile("balanced", &e, &b, &t)
        });
        turbo_btn.connect_clicked({
            let e = e_c.clone();
            let b = b_c.clone();
            let t = t_c.clone();
            move |_| select_profile("performance", &e, &b, &t)
        });

        seg.append(&eco_btn);
        seg.append(&bal_btn);
        seg.append(&turbo_btn);
        content.append(&seg);
    }

    let mut update_fan_lbl_opt: Option<Rc<dyn Fn()>> = None;
    if has_fan_telem {
        // Colorless, subtle fan telemetry info with live refresh
        let fan_lbl = make_label("", "cc-meta", 0.5);
        fan_lbl.set_margin_top(2);
        fan_lbl.set_margin_bottom(2);
        content.append(&fan_lbl);

        let update_fan_lbl = {
            let fan_lbl = fan_lbl.clone();
            Rc::new(move || {
                let (cpu_rpm, gpu_rpm, cpu_pct, gpu_pct) = hw::get_fan_telemetry();
                let c_pct = cpu_pct.unwrap_or(0);
                let g_pct = gpu_pct.unwrap_or(0);
                let is_cb = hw::get_cooler_boost();

                let fan_str = if is_cb {
                    match (cpu_rpm, gpu_rpm) {
                        (Some(cr), Some(gr)) if cr > 0 && gr > 0 => {
                            format!("󰈐 Cooler Boost: CPU {} RPM  ·  GPU {} RPM", cr, gr)
                        }
                        _ => tr(
                            "󰈐 Cooler Boost: maximum speed (100%)",
                            "󰈐 Cooler Boost: максимальные обороты (100%)",
                        )
                        .to_string(),
                    }
                } else {
                    let mut parts = Vec::new();
                    if let Some(cr) = cpu_rpm {
                        if cr > 0 {
                            if cpu_pct.is_some() {
                                parts.push(format!("CPU {}% ({} RPM)", c_pct, cr));
                            } else {
                                parts.push(format!("Fan 1: {} RPM", cr));
                            }
                        } else if cpu_pct.is_some() {
                            parts.push(format!("CPU {}%", c_pct));
                        }
                    } else if cpu_pct.is_some() {
                        parts.push(format!("CPU {}%", c_pct));
                    }

                    if let Some(gr) = gpu_rpm {
                        if gr > 0 {
                            if gpu_pct.is_some() {
                                parts.push(format!("GPU {}% ({} RPM)", g_pct, gr));
                            } else {
                                parts.push(format!("Fan 2: {} RPM", gr));
                            }
                        } else if gpu_pct.is_some() {
                            parts.push(format!("GPU {}%", g_pct));
                        }
                    } else if gpu_pct.is_some() {
                        parts.push(format!("GPU {}%", g_pct));
                    }

                    if parts.is_empty() {
                        tr("󰈨 Fans: active", "󰈨 Кулеры: активный режим").to_string()
                    } else {
                        format!("󰈨 {}: {}", tr("Fans", "Кулеры"), parts.join(" · "))
                    }
                };
                fan_lbl.set_label(&fan_str);
            })
        };
        update_fan_lbl();

        // Live periodic telemetry timer (1 sec) while widget is active/mapped
        let u_fan_t = update_fan_lbl.clone();
        let f_lbl_t = fan_lbl.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(1000), move || {
            if !f_lbl_t.is_mapped() {
                return glib::ControlFlow::Break;
            }
            u_fan_t();
            glib::ControlFlow::Continue
        });
        update_fan_lbl_opt = Some(update_fan_lbl);
    }

    // Чипы Cooler Boost (MSI EC) и Порога батареи (если поддерживается ядром)
    if has_ec || has_bat_thresh {
        let chips = Box::new(Orientation::Horizontal, 8);
        chips.set_homogeneous(true);

        if has_ec {
            let cb_chip = Button::new();
            cb_chip.add_css_class("cc-chip");
            cb_chip.add_css_class("red");

            let cb_active = Rc::new(RefCell::new(hw::get_cooler_boost()));
            let update_cb_ui = {
                let cb_chip = cb_chip.clone();
                let cb_active = cb_active.clone();
                Rc::new(move || {
                    let active = *cb_active.borrow();
                    if active {
                        cb_chip.set_label(&format!("󰈐 {}: {}", tr("Cooler Boost", "Cooler Boost"), tr("on", "вкл")));
                        cb_chip.add_css_class("on");
                    } else {
                        cb_chip.set_label(&format!("󰈐 {}", tr("Cooler Boost", "Cooler Boost")));
                        cb_chip.remove_css_class("on");
                    }
                })
            };
            update_cb_ui();

            let cb_active_c = cb_active.clone();
            let u_cb = update_cb_ui.clone();
            let u_fan_cb = update_fan_lbl_opt.clone();
            cb_chip.connect_clicked(move |_| {
                let cur = *cb_active_c.borrow();
                let next = !cur;
                *cb_active_c.borrow_mut() = next;
                hw::set_cooler_boost(next);
                u_cb();
                if let Some(ref f) = u_fan_cb {
                    f();
                }
            });
            chips.append(&cb_chip);
        }

        if has_bat_thresh {
            let bat_chip = Button::new();
            bat_chip.add_css_class("cc-chip");
            let bat_thresh = Rc::new(RefCell::new(hw::get_battery_threshold()));
            let update_bat_ui = {
                let bat_chip = bat_chip.clone();
                let bat_thresh = bat_thresh.clone();
                Rc::new(move || {
                    let t = *bat_thresh.borrow();
                    if t < 100 {
                        bat_chip.set_label(&format!("󱈑 {}: {}%", tr("Battery Limit", "Лимит заряда"), t));
                        bat_chip.add_css_class("on");
                    } else {
                        bat_chip.set_label(&format!("󱈑 {}", tr("Charge to 100%", "Заряд до 100%")));
                        bat_chip.remove_css_class("on");
                    }
                })
            };
            update_bat_ui();

            let bat_thresh_c = bat_thresh.clone();
            let u_bat = update_bat_ui.clone();
            bat_chip.connect_clicked(move |_| {
                let cur = *bat_thresh_c.borrow();
                let next = if cur < 100 { 100 } else { 80 };
                *bat_thresh_c.borrow_mut() = next;
                hw::set_battery_threshold(next);
                u_bat();
            });
            chips.append(&bat_chip);
        }
        content.append(&chips);
    }

    // ---------------------------------------------------------
    // 5. ГРАФИКА (EnvyControl — только если утилита доступна)
    // ---------------------------------------------------------
    if hw::has_envycontrol() {
        let gpu_row = Box::new(Orientation::Horizontal, 8);
        let gpu_txt = Box::new(Orientation::Vertical, 0);
        gpu_txt.set_hexpand(true);
        gpu_txt.set_valign(Align::Center);
        gpu_txt.append(&make_label(tr("GRAPHICS", "ГРАФИКА"), "cc-label", 0.0));
        let gpu_desc = make_label(tr("Hybrid graphics (iGPU + dGPU)", "Гибридная графика (iGPU + dGPU)"), "cc-foot", 0.0);
        gpu_txt.append(&gpu_desc);
        gpu_row.append(&gpu_txt);

        let gpu_seg = Box::new(Orientation::Horizontal, 2);
        gpu_seg.add_css_class("cc-seg");

        let igpu_btn = Button::with_label("󰢮 iGPU");
        let hybrid_btn = Button::with_label("󰾲 Hybrid");

        let cur_mode = hw::get_gpu_mode();
        if cur_mode == "integrated" {
            igpu_btn.add_css_class("active");
            gpu_desc.set_label(tr("Integrated graphics only (iGPU)", "Только встроенная графика (iGPU)"));
        } else {
            hybrid_btn.add_css_class("active");
            gpu_desc.set_label(tr("iGPU + discrete GPU on demand", "iGPU + дискретная GPU по требованию"));
        }

        let gpu_status = make_label("", "cc-status", 0.0);
        gpu_status.set_wrap(true);
        gpu_status.set_visible(false);

        let reboot_btn = Button::with_label(tr("󰜉  Reboot now", "󰜉  Перезагрузить сейчас"));
        reboot_btn.add_css_class("cc-chip");
        reboot_btn.add_css_class("on");
        reboot_btn.set_visible(false);
        reboot_btn.connect_clicked(|_| {
            let _ = Command::new("systemctl").arg("reboot").spawn();
        });

        let igpu_res = igpu_btn.clone();
        let hyb_res = hybrid_btn.clone();
        let desc_res = gpu_desc.clone();
        let stat_res = gpu_status.clone();
        let reboot_res = reboot_btn.clone();

        let cb = Rc::new(move |ok: bool, target: &'static str| {
            igpu_res.set_sensitive(true);
            hyb_res.set_sensitive(true);
            if ok {
                igpu_res.remove_css_class("active");
                hyb_res.remove_css_class("active");
                if target == "integrated" {
                    igpu_res.add_css_class("active");
                    desc_res.set_label(tr("Integrated graphics only (iGPU)", "Только встроенная графика (iGPU)"));
                } else {
                    hyb_res.add_css_class("active");
                    desc_res.set_label(tr("iGPU + discrete GPU on demand", "iGPU + дискретная GPU по требованию"));
                }
                stat_res.set_label(tr("󰄬 Ready. Mode will apply after reboot.", "󰄬 Готово. Режим применится после перезагрузки."));
                stat_res.add_css_class("ok");
                reboot_res.set_visible(true);
            } else {
                stat_res.set_label(tr("󰅙 Envycontrol switch error", "󰅙 Ошибка переключения envycontrol"));
                stat_res.add_css_class("err");
            }
        });

        GPU_CALLBACK.with(|c| {
            *c.borrow_mut() = Some(cb);
        });

        let igpu_click = igpu_btn.clone();
        let hyb_click = hybrid_btn.clone();
        let stat_click = gpu_status.clone();
        igpu_btn.connect_clicked(move |_| {
            igpu_click.set_sensitive(false);
            hyb_click.set_sensitive(false);
            stat_click.set_visible(true);
            stat_click.set_label("󰑐 Переключение на integrated: пересборка initramfs...");
            thread::spawn(move || {
                let ok = hw::switch_gpu_mode("integrated");
                glib::MainContext::default().invoke(move || {
                    GPU_CALLBACK.with(|cb| {
                        if let Some(ref f) = *cb.borrow() {
                            f(ok, "integrated");
                        }
                    });
                });
            });
        });

        let igpu_click2 = igpu_btn.clone();
        let hyb_click2 = hybrid_btn.clone();
        let stat_click2 = gpu_status.clone();
        hybrid_btn.connect_clicked(move |_| {
            igpu_click2.set_sensitive(false);
            hyb_click2.set_sensitive(false);
            stat_click2.set_visible(true);
            stat_click2.set_label("󰑐 Переключение на hybrid: пересборка initramfs...");
            thread::spawn(move || {
                let ok = hw::switch_gpu_mode("hybrid");
                glib::MainContext::default().invoke(move || {
                    GPU_CALLBACK.with(|cb| {
                        if let Some(ref f) = *cb.borrow() {
                            f(ok, "hybrid");
                        }
                    });
                });
            });
        });

        gpu_seg.append(&igpu_btn);
        gpu_seg.append(&hybrid_btn);
        gpu_row.append(&gpu_seg);
        content.append(&gpu_row);
        content.append(&gpu_status);
        content.append(&reboot_btn);
    }

    let scroll = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .vscrollbar_policy(PolicyType::Automatic)
        .propagate_natural_width(true)
        .vexpand(true)
        .hexpand(true)
        .min_content_height(480)
        .child(&content)
        .build();
    root.append(&scroll);

    (root, 384, 540)
}
