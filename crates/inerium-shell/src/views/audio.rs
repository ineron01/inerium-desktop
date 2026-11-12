use std::rc::Rc;
use gtk4::prelude::*;
use gtk4::{Box, Button, Label, Orientation, Scale, ScrolledWindow};

use crate::hw;

pub fn build_audio_view<FNav, FClose>(
    nav_cb: FNav,
    close_cb: FClose,
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

    let title = Label::new(Some("Аудиоустройства"));
    title.add_css_class("cc-host");
    title.set_hexpand(true);
    title.set_xalign(0.0);
    head.append(&title);

    let pavu_btn = Button::with_label("󰕾 Микшер");
    pavu_btn.add_css_class("cc-chip");
    let c_cb = close_cb.clone();
    pavu_btn.connect_clicked(move |_| {
        c_cb();
        let _ = std::process::Command::new("uwsm")
            .args(["app", "--", "pavucontrol"])
            .spawn()
            .or_else(|_| std::process::Command::new("pavucontrol").spawn());
    });
    head.append(&pavu_btn);
    root.append(&head);

    // Master Volume Row
    let vol_box = Box::new(Orientation::Horizontal, 8);
    vol_box.add_css_class("cc-card");
    let vol_ico = Label::new(Some("󰕾"));
    vol_ico.add_css_class("card-icon");
    vol_box.append(&vol_ico);

    let cur_vol = hw::get_volume();
    let vol_sc = Scale::with_range(Orientation::Horizontal, 0.0, 150.0, 1.0);
    vol_sc.add_css_class("cc-scale");
    vol_sc.set_hexpand(true);
    vol_sc.set_value(cur_vol as f64);
    vol_box.append(&vol_sc);

    let vol_pct = Label::new(Some(&format!("{}%", cur_vol)));
    vol_pct.add_css_class("cc-pct");
    vol_box.append(&vol_pct);

    let v_pct_c = vol_pct.clone();
    vol_sc.connect_value_changed(move |sc| {
        let val = sc.value() as u32;
        v_pct_c.set_label(&format!("{}%", val));
        hw::set_volume(val);
    });
    root.append(&vol_box);

    // Devices List
    let list_title = Label::new(Some("УСТРОЙСТВА ВЫВОДА"));
    list_title.add_css_class("cc-label");
    list_title.set_xalign(0.0);
    root.append(&list_title);

    let scroll = ScrolledWindow::new();
    scroll.add_css_class("list-scroll");
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroll.set_propagate_natural_height(true);
    scroll.set_min_content_height(140);
    scroll.set_max_content_height(260);
    scroll.set_vexpand(true);

    let list_box = Box::new(Orientation::Vertical, 4);

    let sinks = hw::list_audio_sinks();
    let sink_buttons = Rc::new(std::cell::RefCell::new(Vec::<(Button, Label)>::new()));

    for sink in sinks {
        let row = Button::new();
        row.add_css_class("card-item");
        if sink.is_default {
            row.add_css_class("active");
        }

        let inner = Box::new(Orientation::Horizontal, 8);
        let ico = Label::new(Some(if sink.name.to_lowercase().contains("headphone") || sink.name.to_lowercase().contains("buds") { "󰋋" } else { "󰕾" }));
        ico.add_css_class("card-icon");
        inner.append(&ico);

        let txt = Box::new(Orientation::Vertical, 1);
        txt.set_hexpand(true);
        let name_lbl = Label::new(Some(&sink.name));
        name_lbl.add_css_class("card-title");
        name_lbl.set_xalign(0.0);
        name_lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        txt.append(&name_lbl);
        inner.append(&txt);

        let badge = Label::new(Some(if sink.is_default { "󰄬 Активен" } else { "" }));
        badge.add_css_class("card-badge");
        inner.append(&badge);

        row.set_child(Some(&inner));

        let s_id = sink.id;
        let s_btns = sink_buttons.clone();
        let r_clone = row.clone();
        let b_clone = badge.clone();
        row.connect_clicked(move |_| {
            hw::set_default_sink(s_id);
            for (btn, bg) in s_btns.borrow().iter() {
                btn.remove_css_class("active");
                bg.set_label("");
            }
            r_clone.add_css_class("active");
            b_clone.set_label("󰄬 Активен");
        });

        sink_buttons.borrow_mut().push((row.clone(), badge));
        list_box.append(&row);
    }

    scroll.set_child(Some(&list_box));
    root.append(&scroll);

    (root, 380, 320)
}
