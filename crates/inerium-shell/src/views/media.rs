use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;
use std::thread;

use gtk4::prelude::*;
use gtk4::{Align, Box, Button, GestureClick, Label, Orientation, Picture, ProgressBar};

#[derive(Clone, Default)]
pub struct MediaInfo {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub player_name: String,
    pub art_url: String,
    pub position_str: String,
    pub length_str: String,
    pub position_us: u64,
    pub length_us: u64,
    pub is_playing: bool,
}

fn query_media_info() -> Option<MediaInfo> {
    let out = Command::new("playerctl")
        .args([
            "metadata",
            "--format",
            "{{title}}@|@{{artist}}@|@{{album}}@|@{{mpris:artUrl}}@|@{{duration(position)}}@|@{{duration(mpris:length)}}@|@{{status}}@|@{{playerName}}@|@{{position}}@|@{{mpris:length}}",
        ])
        .output()
        .ok()?;

    if !out.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if text.is_empty() {
        return None;
    }

    let parts: Vec<&str> = text.split("@|@").collect();
    if parts.len() < 7 {
        return None;
    }

    let player_name = if parts.len() > 7 { parts[7].trim().to_string() } else { String::new() };
    let position_us = if parts.len() > 8 { parts[8].trim().parse::<u64>().unwrap_or(0) } else { 0 };
    let length_us = if parts.len() > 9 { parts[9].trim().parse::<u64>().unwrap_or(0) } else { 0 };

    Some(MediaInfo {
        title: parts[0].trim().to_string(),
        artist: parts[1].trim().to_string(),
        album: parts[2].trim().to_string(),
        art_url: parts[3].trim().to_string(),
        position_str: parts[4].trim().to_string(),
        length_str: parts[5].trim().to_string(),
        is_playing: parts[6].trim().eq_ignore_ascii_case("playing"),
        player_name,
        position_us,
        length_us,
    })
}

fn get_art_cache_path(url: &str) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    let hash = hasher.finish();
    PathBuf::from(format!("/tmp/inerium_art_{:x}.png", hash))
}

fn get_player_badge(name: &str) -> (&'static str, String) {
    let lower = name.to_lowercase();
    if lower.contains("chrom") {
        ("󰊯", "Chromium".to_string())
    } else if lower.contains("firef") {
        ("󰈹", "Firefox".to_string())
    } else if lower.contains("spot") {
        ("󰓇", "Spotify".to_string())
    } else if lower.contains("mpv") {
        ("󰐹", "MPV".to_string())
    } else if !name.is_empty() {
        ("󰎆", name.to_string())
    } else {
        ("󰎆", "Медиа".to_string())
    }
}

type MediaCallback = Rc<dyn Fn(MediaInfo)>;

thread_local! {
    static MEDIA_CALLBACK: RefCell<Option<MediaCallback>> = RefCell::new(None);
}

pub fn build_media_view<FNav, FClose>(
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

    // -------------------------------------------------------------
    // 1. Header (Back, Player Badge, Status Pill)
    // -------------------------------------------------------------
    let head = Box::new(Orientation::Horizontal, 8);
    let back_btn = Button::with_label("‹ Назад");
    back_btn.add_css_class("back-btn");
    let n_cb = nav_cb.clone();
    back_btn.connect_clicked(move |_| n_cb("control"));
    head.append(&back_btn);

    let player_badge = Label::new(Some("󰎆 Медиаплеер"));
    player_badge.add_css_class("media-badge");
    player_badge.set_halign(Align::Start);
    head.append(&player_badge);

    let spacer = Box::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    head.append(&spacer);

    let status_pill = Label::new(Some("Ожидание"));
    status_pill.add_css_class("media-status-pill");
    head.append(&status_pill);

    root.append(&head);

    // -------------------------------------------------------------
    // 2. Track Info Card (Art, Title, Artist, Album)
    // -------------------------------------------------------------
    let card = Box::new(Orientation::Horizontal, 12);
    card.add_css_class("media-card");

    let art_box = Box::new(Orientation::Vertical, 0);
    art_box.set_size_request(64, 64);
    art_box.set_valign(Align::Center);
    art_box.add_css_class("media-art-box");

    let art_pic = Picture::new();
    art_pic.set_size_request(64, 64);
    art_pic.set_content_fit(gtk4::ContentFit::Contain);
    art_pic.set_can_shrink(true);
    art_pic.add_css_class("media-art");

    let art_fallback = Label::new(Some("󰎆"));
    art_fallback.add_css_class("media-art-fallback");
    art_fallback.set_valign(Align::Center);
    art_fallback.set_halign(Align::Center);

    art_box.append(&art_pic);
    art_box.append(&art_fallback);
    card.append(&art_box);

    let meta_box = Box::new(Orientation::Vertical, 3);
    meta_box.set_valign(Align::Center);
    meta_box.set_hexpand(true);

    let track_title = Label::new(Some("Ничего не воспроизводится"));
    track_title.add_css_class("media-title");
    track_title.set_xalign(0.0);
    track_title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    track_title.set_max_width_chars(25);
    meta_box.append(&track_title);

    let track_artist = Label::new(Some("Ожидание медиапотока..."));
    track_artist.add_css_class("media-artist");
    track_artist.set_xalign(0.0);
    track_artist.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    track_artist.set_max_width_chars(27);
    meta_box.append(&track_artist);

    card.append(&meta_box);
    root.append(&card);

    // -------------------------------------------------------------
    // 3. Progress Bar & Time Timestamps
    // -------------------------------------------------------------
    let progress_box = Box::new(Orientation::Vertical, 4);
    progress_box.set_margin_start(4);
    progress_box.set_margin_end(4);

    let progress_bar = ProgressBar::new();
    progress_bar.add_css_class("media-progress");
    progress_bar.set_fraction(0.0);

    let current_length_us = Rc::new(RefCell::new(0u64));
    let len_click_ref = current_length_us.clone();

    // Click on progress bar to seek
    let click_gesture = GestureClick::new();
    click_gesture.connect_pressed(move |gesture, _, x, _| {
        let total_us = *len_click_ref.borrow();
        if total_us > 0
            && let Some(widget) = gesture.widget() {
                let width = widget.width() as f64;
                if width > 0.0 {
                    let frac = (x / width).clamp(0.0, 1.0);
                    let target_sec = (total_us as f64 / 1_000_000.0) * frac;
                    thread::spawn(move || {
                        let _ = Command::new("playerctl")
                            .args(["position", &format!("{:.1}", target_sec)])
                            .output();
                    });
                }
            }
    });
    progress_bar.add_controller(click_gesture);
    progress_box.append(&progress_bar);

    let time_box = Box::new(Orientation::Horizontal, 0);
    let time_pos = Label::new(Some("0:00"));
    time_pos.add_css_class("media-time");
    time_pos.set_halign(Align::Start);
    time_box.append(&time_pos);

    let time_spacer = Box::new(Orientation::Horizontal, 0);
    time_spacer.set_hexpand(true);
    time_box.append(&time_spacer);

    let time_len = Label::new(Some("0:00"));
    time_len.add_css_class("media-time");
    time_len.set_halign(Align::End);
    time_box.append(&time_len);

    progress_box.append(&time_box);
    root.append(&progress_box);

    // -------------------------------------------------------------
    // 4. Playback Controls (Prev, Play/Pause, Next)
    // -------------------------------------------------------------
    let ctrl_box = Box::new(Orientation::Horizontal, 14);
    ctrl_box.set_halign(Align::Center);
    ctrl_box.set_margin_top(2);
    ctrl_box.set_margin_bottom(2);

    let prev_btn = Button::with_label("󰒮");
    prev_btn.add_css_class("media-btn");
    ctrl_box.append(&prev_btn);

    let play_btn = Button::with_label("󰐊");
    play_btn.add_css_class("play-btn");
    ctrl_box.append(&play_btn);

    let next_btn = Button::with_label("󰒭");
    next_btn.add_css_class("media-btn");
    ctrl_box.append(&next_btn);

    root.append(&ctrl_box);

    // -------------------------------------------------------------
    // 5. Reactive State & UI Binding
    // -------------------------------------------------------------
    let title_ui = track_title.clone();
    let artist_ui = track_artist.clone();
    let pic_ui = art_pic.clone();
    let fb_ui = art_fallback.clone();
    let play_ui = play_btn.clone();
    let p_bar_ui = progress_bar.clone();
    let pos_ui = time_pos.clone();
    let len_ui = time_len.clone();
    let badge_ui = player_badge.clone();
    let pill_ui = status_pill.clone();

    let is_playing_state = Rc::new(RefCell::new(false));
    let last_art_url = Rc::new(RefCell::new(String::new()));
    let len_state = current_length_us.clone();

    let is_p_ref = is_playing_state.clone();
    let last_art_ref = last_art_url.clone();

    let apply_info = Rc::new(move |info: MediaInfo| {
        *is_p_ref.borrow_mut() = info.is_playing;
        *len_state.borrow_mut() = info.length_us;

        // Play/Pause button
        play_ui.set_label(if info.is_playing { "󰏤" } else { "󰐊" });

        // Player Badge & Status Pill
        let (ico, badge_text) = get_player_badge(&info.player_name);
        badge_ui.set_label(&format!("{} {}", ico, badge_text));

        if info.is_playing {
            pill_ui.set_label("󰐊 Играет");
            pill_ui.remove_css_class("paused");
        } else {
            pill_ui.set_label("󰏤 Пауза");
            pill_ui.add_css_class("paused");
        }

        // Title & Artist/Album
        if info.title.is_empty() {
            title_ui.set_label("Ничего не воспроизводится");
            artist_ui.set_label("Ожидание медиапотока...");
            pos_ui.set_label("0:00");
            len_ui.set_label("0:00");
            p_bar_ui.set_fraction(0.0);
            pic_ui.set_visible(false);
            fb_ui.set_visible(true);
            return;
        }

        title_ui.set_label(&info.title);

        let sub_text = if !info.album.is_empty() && info.album != info.title && info.album != info.artist {
            format!("{} · {}", info.artist, info.album)
        } else if !info.artist.is_empty() {
            info.artist.clone()
        } else {
            "Неизвестный исполнитель".to_string()
        };
        artist_ui.set_label(&sub_text);

        // Timestamps & Progress Fraction
        pos_ui.set_label(if !info.position_str.is_empty() { &info.position_str } else { "0:00" });
        len_ui.set_label(if !info.length_str.is_empty() { &info.length_str } else { "0:00" });

        let frac = if info.length_us > 0 {
            (info.position_us as f64 / info.length_us as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };
        p_bar_ui.set_fraction(frac);

        // Album Art Handling
        let url = info.art_url.clone();
        if url.is_empty() {
            pic_ui.set_visible(false);
            fb_ui.set_visible(true);
            *last_art_ref.borrow_mut() = String::new();
        } else if *last_art_ref.borrow() != url {
            *last_art_ref.borrow_mut() = url.clone();

            if let Some(file_path) = url.strip_prefix("file://") {
                let p = std::path::Path::new(file_path);
                if p.exists() {
                    pic_ui.set_filename(Some(p));
                    pic_ui.set_visible(true);
                    fb_ui.set_visible(false);
                } else {
                    pic_ui.set_visible(false);
                    fb_ui.set_visible(true);
                }
            } else if url.starts_with("http://") || url.starts_with("https://") {
                let cache_p = get_art_cache_path(&url);
                if cache_p.exists() {
                    pic_ui.set_filename(Some(&cache_p));
                    pic_ui.set_visible(true);
                    fb_ui.set_visible(false);
                } else {
                    pic_ui.set_visible(false);
                    fb_ui.set_visible(true);
                    let target_url = url.clone();
                    let target_path = cache_p.clone();
                    thread::spawn(move || {
                        let _ = Command::new("curl")
                            .args(["-s", "-L", "-o", target_path.to_str().unwrap_or(""), &target_url, "--max-time", "2"])
                            .output();
                        if target_path.exists() {
                            glib::MainContext::default().invoke(move || {
                                MEDIA_CALLBACK.with(|cb| {
                                    if let Some(ref f) = *cb.borrow()
                                        && let Some(inf) = query_media_info() {
                                            f(inf);
                                        }
                                });
                            });
                        }
                    });
                }
            } else {
                pic_ui.set_visible(false);
                fb_ui.set_visible(true);
            }
        }
    });

    // Initial query
    if let Some(info) = query_media_info() {
        apply_info(info);
    } else {
        apply_info(MediaInfo::default());
    }

    // Register callback for async events
    let apply_cb = apply_info.clone();
    let cb = Rc::new(move |info: MediaInfo| {
        apply_cb(info);
    });
    MEDIA_CALLBACK.with(|c| {
        *c.borrow_mut() = Some(cb);
    });

    // Live position and status timer (1 sec) while mapped
    let root_mapped = root.clone();
    let apply_timer = apply_info.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(1000), move || {
        if !root_mapped.is_mapped() {
            return glib::ControlFlow::Break;
        }
        if let Some(info) = query_media_info() {
            apply_timer(info);
        }
        glib::ControlFlow::Continue
    });

    // Previous button
    prev_btn.connect_clicked(move |_| {
        thread::spawn(move || {
            let _ = Command::new("playerctl").arg("previous").output();
            thread::sleep(std::time::Duration::from_millis(150));
            if let Some(info) = query_media_info() {
                glib::MainContext::default().invoke(move || {
                    MEDIA_CALLBACK.with(|cb| {
                        if let Some(ref f) = *cb.borrow() {
                            f(info);
                        }
                    });
                });
            }
        });
    });

    // Play/Pause button with instant optimistic UI toggle
    let play_click = play_btn.clone();
    let is_p_click = is_playing_state.clone();
    let pill_click = status_pill.clone();
    play_btn.connect_clicked(move |_| {
        let cur = *is_p_click.borrow();
        let next = !cur;
        *is_p_click.borrow_mut() = next;
        play_click.set_label(if next { "󰏤" } else { "󰐊" });
        if next {
            pill_click.set_label("󰐊 Играет");
            pill_click.remove_css_class("paused");
        } else {
            pill_click.set_label("󰏤 Пауза");
            pill_click.add_css_class("paused");
        }

        thread::spawn(move || {
            let _ = Command::new("playerctl").arg("play-pause").output();
            thread::sleep(std::time::Duration::from_millis(150));
            if let Some(info) = query_media_info() {
                glib::MainContext::default().invoke(move || {
                    MEDIA_CALLBACK.with(|cb| {
                        if let Some(ref f) = *cb.borrow() {
                            f(info);
                        }
                    });
                });
            }
        });
    });

    // Next button
    next_btn.connect_clicked(move |_| {
        thread::spawn(move || {
            let _ = Command::new("playerctl").arg("next").output();
            thread::sleep(std::time::Duration::from_millis(150));
            if let Some(info) = query_media_info() {
                glib::MainContext::default().invoke(move || {
                    MEDIA_CALLBACK.with(|cb| {
                        if let Some(ref f) = *cb.borrow() {
                            f(info);
                        }
                    });
                });
            }
        });
    });

    (root, 390, 210)
}
