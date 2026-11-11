use std::process::Command;
use std::rc::Rc;
use std::thread;

use gdk4::Key;
use gtk4::prelude::*;
use gtk4::{Align, Box, Button, EventControllerKey, GestureClick, Label, Orientation};

pub fn build_power_menu<FClose>(close_cb: FClose) -> (Box, i32, i32)
where
    FClose: Fn() + 'static + Clone,
{
    // Outer fullscreen container covering the blurred display
    let root = Box::new(Orientation::Vertical, 0);
    root.add_css_class("power-backdrop");
    root.set_hexpand(true);
    root.set_vexpand(true);
    root.set_halign(Align::Fill);
    root.set_valign(Align::Fill);

    // Inner modal box
    let modal = Box::new(Orientation::Vertical, 20);
    modal.add_css_class("power-modal");
    modal.set_size_request(840, -1);
    modal.set_hexpand(true);
    modal.set_vexpand(true);
    modal.set_halign(Align::Center);
    modal.set_valign(Align::Center);

    // Header (Title + Subtitle)
    let header_box = Box::new(Orientation::Vertical, 4);
    header_box.set_halign(Align::Center);

    let title = Label::new(Some("Управление питанием"));
    title.add_css_class("power-title");
    title.set_halign(Align::Center);
    header_box.append(&title);

    let subtitle = Label::new(Some("Нажмите 1–5 на клавиатуре или выберите действие"));
    subtitle.add_css_class("power-subtitle");
    subtitle.set_halign(Align::Center);
    header_box.append(&subtitle);

    modal.append(&header_box);

    // Grid of 5 Cards
    let grid = Box::new(Orientation::Horizontal, 14);
    grid.set_homogeneous(true);

    fn make_power_card<FAct>(
        icon: &str,
        label: &str,
        key_num: &str,
        is_danger: bool,
        action: FAct,
    ) -> Button
    where
        FAct: Fn() + 'static,
    {
        let btn = Button::new();
        btn.add_css_class("power-card");
        if is_danger {
            btn.add_css_class("danger");
        }

        let inner = Box::new(Orientation::Vertical, 6);
        inner.set_halign(Align::Center);
        inner.set_valign(Align::Center);

        let ico = Label::new(Some(icon));
        ico.add_css_class("power-card-ico");
        inner.append(&ico);

        let txt = Label::new(Some(label));
        txt.add_css_class("power-card-label");
        inner.append(&txt);

        let key_badge = Label::new(Some(key_num));
        key_badge.add_css_class("power-card-key");
        key_badge.set_halign(Align::Center);
        key_badge.set_valign(Align::Center);
        inner.append(&key_badge);

        btn.set_child(Some(&inner));
        btn.connect_clicked(move |_| action());
        btn
    }

    // Actions
    let c1 = close_cb.clone();
    let act_lock: Rc<dyn Fn()> = Rc::new(move || {
        c1();
        thread::spawn(|| {
            let _ = Command::new("sh").args(["-c", "hyprlock || loginctl lock-session"]).output();
        });
    });

    let c2 = close_cb.clone();
    let act_logout: Rc<dyn Fn()> = Rc::new(move || {
        c2();
        thread::spawn(|| {
            let _ = Command::new("sh")
                .args(["-c", "if uwsm check is-active >/dev/null 2>&1; then uwsm stop; else env -u HYPRLAND_INSTANCE_SIGNATURE hyprctl dispatch exit 2>/dev/null || hyprctl dispatch exit 2>/dev/null || loginctl terminate-session \"$XDG_SESSION_ID\"; fi"])
                .output();
        });
    });

    let c3 = close_cb.clone();
    let act_suspend: Rc<dyn Fn()> = Rc::new(move || {
        c3();
        thread::spawn(|| {
            let _ = Command::new("systemctl").arg("suspend").output();
        });
    });

    let c4 = close_cb.clone();
    let act_reboot: Rc<dyn Fn()> = Rc::new(move || {
        c4();
        thread::spawn(|| {
            let _ = Command::new("systemctl").arg("reboot").output();
        });
    });

    let c5 = close_cb.clone();
    let act_poweroff: Rc<dyn Fn()> = Rc::new(move || {
        c5();
        thread::spawn(|| {
            let _ = Command::new("systemctl").arg("poweroff").output();
        });
    });

    let c_cancel = close_cb.clone();
    let act_cancel: Rc<dyn Fn()> = Rc::new(move || {
        c_cancel();
    });

    // 1. Lock
    let a1 = act_lock.clone();
    let lock_card = make_power_card("󰌾", "Блокировка", "1", false, move || a1());
    grid.append(&lock_card);

    // 2. Logout
    let a2 = act_logout.clone();
    let logout_card = make_power_card("󰍃", "Выход", "2", false, move || a2());
    grid.append(&logout_card);

    // 3. Suspend
    let a3 = act_suspend.clone();
    let sleep_card = make_power_card("󰤄", "Сон", "3", false, move || a3());
    grid.append(&sleep_card);

    // 4. Reboot
    let a4 = act_reboot.clone();
    let reboot_card = make_power_card("󰜉", "Перезагрузка", "4", false, move || a4());
    grid.append(&reboot_card);

    // 5. Shutdown
    let a5 = act_poweroff.clone();
    let poweroff_card = make_power_card("⏻", "Выключение", "5", true, move || a5());
    grid.append(&poweroff_card);

    modal.append(&grid);

    // Cancel Button
    let cancel_btn = Button::with_label("Отмена (Esc)");
    cancel_btn.add_css_class("cc-chip");
    cancel_btn.add_css_class("power-cancel-btn");
    cancel_btn.set_halign(Align::Center);
    let a_c = act_cancel.clone();
    cancel_btn.connect_clicked(move |_| a_c());
    modal.append(&cancel_btn);

    root.append(&modal);

    // Keyboard controller for keys 1..5 and Esc
    let key_ctrl = EventControllerKey::new();
    let k_lock = act_lock.clone();
    let k_logout = act_logout.clone();
    let k_suspend = act_suspend.clone();
    let k_reboot = act_reboot.clone();
    let k_poweroff = act_poweroff.clone();
    let k_cancel = act_cancel.clone();

    key_ctrl.connect_key_pressed(move |_, key, _, _| {
        let mut handled = true;
        match key {
            Key::_1 | Key::KP_1 => k_lock(),
            Key::_2 | Key::KP_2 => k_logout(),
            Key::_3 | Key::KP_3 => k_suspend(),
            Key::_4 | Key::KP_4 => k_reboot(),
            Key::_5 | Key::KP_5 => k_poweroff(),
            Key::Escape => k_cancel(),
            _ => {
                if let Some(c) = key.to_unicode() {
                    match c {
                        '1' => k_lock(),
                        '2' => k_logout(),
                        '3' => k_suspend(),
                        '4' => k_reboot(),
                        '5' => k_poweroff(),
                        _ => {
                            handled = false;
                        }
                    }
                } else {
                    handled = false;
                }
            }
        }
        if handled {
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    root.add_controller(key_ctrl);

    // GestureClick to close on clicking the blurred backdrop outside modal
    let modal_weak = modal.downgrade();
    let c_outside = act_cancel.clone();
    let click_gesture = GestureClick::new();
    click_gesture.connect_released(move |gesture, _, x, y| {
        if let Some(target) = gesture.widget()
            && let Some(m) = modal_weak.upgrade()
                && let Some(rect) = m.compute_bounds(&target) {
                    let rx = rect.x() as f64;
                    let ry = rect.y() as f64;
                    let rw = rect.width() as f64;
                    let rh = rect.height() as f64;
                    if x >= rx && x <= rx + rw && y >= ry && y <= ry + rh {
                        // Inside modal, ignore
                        return;
                    }
                }
        c_outside();
    });
    root.add_controller(click_gesture);

    (root, 840, 320)
}
