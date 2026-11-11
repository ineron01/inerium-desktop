#[allow(dead_code)]
pub mod palette {
    pub const BG: &str = "rgba(24, 22, 22, 0.97)";
    pub const SURFACE: &str = "#282727";
    pub const LINE: &str = "rgba(197, 201, 197, 0.12)";
    pub const HOVER: &str = "rgba(197, 201, 197, 0.08)";
    pub const FG: &str = "#c5c9c5";
    pub const FG_BRIGHT: &str = "#e6e2cc";
    pub const MUTED: &str = "#737c73";
    pub const SUBTLE: &str = "#a6a69c";
    pub const GREEN: &str = "#87a987";
    pub const BLUE: &str = "#8ba4b0";
    pub const AQUA: &str = "#8ea4a2";
    pub const YELLOW: &str = "#c4b28a";
    pub const ORANGE: &str = "#b6927b";
    pub const RED: &str = "#c4746e";
    pub const VIOLET: &str = "#8992a7";
}

pub const POPUP_CSS: &str = r#"
@define-color bg          rgba(24, 22, 22, 0.97);
@define-color surface     #282727;
@define-color line        rgba(197, 201, 197, 0.12);
@define-color hover       rgba(197, 201, 197, 0.08);
@define-color fg          #c5c9c5;
@define-color fg-bright   #e6e2cc;
@define-color muted       #737c73;
@define-color subtle      #a6a69c;
@define-color green       #87a987;
@define-color blue        #8ba4b0;
@define-color aqua        #8ea4a2;
@define-color yellow      #c4b28a;
@define-color orange      #b6927b;
@define-color red         #c4746e;
@define-color violet      #8992a7;

window.popup-window {
    background: transparent;
}

.popup-container, .cc {
    background-color: rgba(24, 22, 22, 0.97);
    border: 1px solid rgba(197, 201, 197, 0.12);
    border-radius: 16px;
    padding: 14px;
    box-shadow: 0 12px 36px rgba(0, 0, 0, 0.85);
    font-family: "JetBrainsMono Nerd Font", monospace;
    color: #c5c9c5;
}

/* Header */
.cc-head {
    margin-bottom: 6px;
}
.cc-host {
    font-size: 14px;
    font-weight: 700;
    color: #e6e2cc;
}
.cc-meta {
    font-size: 11px;
    color: #737c73;
}
.cc-meta-accent {
    font-size: 11px;
    color: #87a987;
    font-weight: 600;
}
.cc-meta-btn {
    padding: 1px 6px;
    margin: 0;
    border-radius: 6px;
    border: none;
    background-color: transparent;
    color: #737c73;
    font-size: 11px;
    font-weight: 500;
    transition: background-color 150ms ease, color 150ms ease;
}
.cc-meta-btn:hover {
    background-color: rgba(197, 201, 197, 0.08);
    color: #e6e2cc;
}
.cc-meta-btn.has-updates {
    color: #8ea4a2;
    background-color: rgba(142, 164, 162, 0.08);
}
.cc-meta-btn.has-updates:hover {
    color: #e6e2cc;
    background-color: rgba(142, 164, 162, 0.18);
}
.cc-meta-btn-lbl {
    font-size: 11px;
    font-weight: 600;
}
.cc-icon-btn {
    min-width: 32px;
    min-height: 32px;
    padding: 0;
    border-radius: 10px;
    border: none;
    background-color: rgba(197, 201, 197, 0.06);
    color: #a6a69c;
    font-size: 14px;
}
.cc-icon-btn:hover {
    background-color: rgba(197, 201, 197, 0.14);
    color: #e6e2cc;
}
.cc-icon-btn.danger {
    color: #c4746e;
}
.cc-icon-btn.danger:hover {
    background-color: rgba(196, 116, 110, 0.22);
    color: #e6e2cc;
}

/* Dedicated Back Button */
.back-btn {
    min-width: 72px;
    min-height: 28px;
    padding: 4px 12px;
    border-radius: 8px;
    border: 1px solid rgba(197, 201, 197, 0.12);
    background-color: rgba(197, 201, 197, 0.06);
    color: #87a987;
    font-size: 12px;
    font-weight: 600;
}
.back-btn:hover {
    background-color: rgba(135, 169, 135, 0.20);
    border-color: rgba(135, 169, 135, 0.40);
    color: #e6e2cc;
}

/* Fan Telemetry Badge */
.cc-fan-badge {
    padding: 4px 8px;
    border-radius: 8px;
    background-color: rgba(197, 201, 197, 0.05);
    border: 1px solid rgba(197, 201, 197, 0.08);
    font-size: 10.5px;
    color: #87a987;
    font-weight: 600;
}

/* Quick Action Tiles */
.cc-tile {
    border-radius: 14px;
    background-color: #221f1f;
    border: 1px solid rgba(197, 201, 197, 0.07);
}
.cc-tile-main, .cc-tile-more {
    border: none;
    background: none;
    box-shadow: none;
    border-radius: 14px;
    padding: 8px 10px;
}
.cc-tile-more {
    padding: 8px 10px;
    color: #737c73;
    font-size: 15px;
}
.cc-tile-main:hover, .cc-tile-more:hover {
    background-color: rgba(197, 201, 197, 0.07);
}
.cc-tile-ico {
    min-width: 32px;
    min-height: 32px;
    border-radius: 10px;
    background-color: rgba(197, 201, 197, 0.07);
    color: #a6a69c;
    font-size: 15px;
}
.cc-tile-title {
    font-size: 12px;
    font-weight: 700;
    color: #c5c9c5;
}
.cc-tile-sub {
    font-size: 10.5px;
    color: #737c73;
}

.cc-tile.on {
    background-color: rgba(135, 169, 135, 0.16);
    border-color: rgba(135, 169, 135, 0.35);
}
.cc-tile.on .cc-tile-ico {
    background-color: #87a987;
    color: #181616;
}
.cc-tile.on .cc-tile-sub {
    color: #a6a69c;
}

.cc-tile.on.blue {
    background-color: rgba(139, 164, 176, 0.16);
    border-color: rgba(139, 164, 176, 0.38);
}
.cc-tile.on.blue .cc-tile-ico {
    background-color: #8ba4b0;
    color: #181616;
}

.cc-tile.on.amber {
    background-color: rgba(196, 178, 138, 0.15);
    border-color: rgba(196, 178, 138, 0.38);
}
.cc-tile.on.amber .cc-tile-ico {
    background-color: #c4b28a;
    color: #181616;
}

.cc-tile.on.red {
    background-color: rgba(196, 116, 110, 0.15);
    border-color: rgba(196, 116, 110, 0.38);
}
.cc-tile.on.red .cc-tile-ico {
    background-color: #c4746e;
    color: #181616;
}

/* Sliders Card */
.cc-card {
    border-radius: 14px;
    padding: 8px 10px;
    background-color: #221f1f;
    border: 1px solid rgba(197, 201, 197, 0.07);
}
.cc-label {
    font-size: 10.5px;
    font-weight: 700;
    color: #737c73;
    letter-spacing: 1px;
}
.cc-slider-ico {
    min-width: 28px;
    min-height: 28px;
    padding: 0;
    border: none;
    border-radius: 8px;
    background: none;
    color: #a6a69c;
    font-size: 14px;
}
.cc-slider-ico:hover {
    background-color: rgba(197, 201, 197, 0.08);
    color: #e6e2cc;
}
.cc-slider-ico.muted {
    color: #c4746e;
}
.cc-pct {
    font-size: 11px;
    color: #a6a69c;
    min-width: 38px;
}
.cc-scale trough {
    min-height: 6px;
    border-radius: 99px;
    background-color: rgba(197, 201, 197, 0.10);
}
.cc-scale highlight {
    border-radius: 99px;
    background-color: #87a987;
}
.cc-scale.amber highlight {
    background-color: #c4b28a;
}
.cc-scale slider {
    min-width: 14px;
    min-height: 14px;
    margin: -5px;
    border-radius: 99px;
    background-color: #e6e2cc;
    border: none;
    box-shadow: none;
}

/* Segmented Buttons */
.cc-seg {
    border-radius: 12px;
    padding: 3px;
    background-color: #221f1f;
    border: 1px solid rgba(197, 201, 197, 0.07);
}
.cc-seg button {
    border: none;
    background: none;
    box-shadow: none;
    border-radius: 9px;
    padding: 6px 4px;
    font-size: 11.5px;
    font-weight: 600;
    color: #a6a69c;
}
.cc-seg button:hover {
    background-color: rgba(197, 201, 197, 0.06);
    color: #e6e2cc;
}
.cc-seg button.active {
    background-color: rgba(135, 169, 135, 0.22);
    color: #e6e2cc;
}
.cc-seg button.active.eco {
    background-color: rgba(142, 164, 162, 0.24);
}
.cc-seg button.active.turbo {
    background-color: rgba(196, 116, 110, 0.24);
}

/* Action Chips */
.cc-chip {
    border-radius: 10px;
    padding: 6px 10px;
    border: 1px solid rgba(197, 201, 197, 0.07);
    background-color: #221f1f;
    box-shadow: none;
    font-size: 11px;
    font-weight: 600;
    color: #a6a69c;
}
.cc-chip:hover {
    background-color: rgba(197, 201, 197, 0.07);
    color: #e6e2cc;
}
.cc-chip.on {
    background-color: rgba(135, 169, 135, 0.18);
    border-color: rgba(135, 169, 135, 0.35);
    color: #e6e2cc;
}
.cc-chip.on.red {
    background-color: rgba(196, 116, 110, 0.18);
    border-color: rgba(196, 116, 110, 0.38);
    color: #e6e2cc;
}
.cc-chip:disabled {
    opacity: 0.70;
    color: #c4b28a;
}

.cc-foot {
    font-size: 10.5px;
    color: #737c73;
}

.cc-status {
    font-size: 11px;
    color: #c4b28a;
}
.cc-status.ok {
    color: #87a987;
}
.cc-status.err {
    color: #c4746e;
}

/* Card list views (Audio, Bluetooth, Network) */
.list-scroll {
}
.card-item {
    background-color: #221f1f;
    border: 1px solid rgba(200, 192, 180, 0.08);
    border-radius: 9px;
    padding: 6px 10px;
    margin-bottom: 4px;
}
.card-item:hover {
    background-color: #2d2a2a;
    border-color: rgba(200, 192, 180, 0.18);
}
.card-item.active {
    background-color: rgba(135, 169, 135, 0.16);
    border-color: rgba(135, 169, 135, 0.55);
}
.card-item:disabled {
    opacity: 0.70;
}
.card-icon {
    font-size: 15px;
    margin-right: 8px;
    color: #87a987;
}
.card-title {
    font-size: 11.5px;
    font-weight: 600;
    color: #c5c9c5;
}
.card-sub {
    font-size: 9.5px;
    color: #8a8980;
}
.card-badge {
    font-size: 9.5px;
    font-weight: 600;
    border-radius: 6px;
    padding: 1.5px 6px;
    background-color: rgba(135, 169, 135, 0.20);
    color: #87a987;
}
.card-badge.active {
    background-color: rgba(135, 169, 135, 0.25);
    color: #87a987;
}
.card-badge.saved {
    background-color: rgba(139, 164, 176, 0.18);
    color: #8ba4b0;
}
.auth-error {
    font-size: 11px;
    color: #c4746e;
    font-weight: 600;
    margin-top: 4px;
    margin-bottom: 2px;
}

/* Modal Power Menu & Fullscreen Backdrop */
window.power-window {
    background-color: rgba(18, 16, 16, 0.48);
}

.power-backdrop {
    background: transparent;
}

.power-modal {
    background-color: rgba(24, 22, 22, 0.96);
    border: 1px solid rgba(197, 201, 197, 0.15);
    border-radius: 24px;
    padding: 32px 36px;
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.9);
}

.power-title {
    font-size: 18px;
    font-weight: 700;
    color: #e6e2cc;
    margin-bottom: 2px;
}

.power-subtitle {
    font-size: 12px;
    color: #727169;
    margin-bottom: 8px;
}

.power-card {
    min-width: 130px;
    min-height: 135px;
    padding: 16px 12px;
    border-radius: 18px;
    background-color: #221f1f;
    border: 1px solid rgba(197, 201, 197, 0.09);
    color: #c5c9c5;
    transition: all 0.2s ease;
}

.power-card:hover {
    background-color: #2d2a2a;
    border-color: rgba(135, 169, 135, 0.45);
    color: #e6e2cc;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
}

.power-card.danger:hover {
    border-color: rgba(196, 116, 110, 0.6);
}

.power-card-ico {
    font-size: 42px;
    margin-bottom: 8px;
}

.power-card-label {
    font-size: 13px;
    font-weight: 600;
    color: #c5c9c5;
}

.power-card:hover .power-card-label {
    color: #e6e2cc;
}

.power-card-key {
    font-size: 11px;
    font-weight: 700;
    color: #8ba4b0;
    background-color: rgba(139, 164, 176, 0.12);
    border: 1px solid rgba(139, 164, 176, 0.22);
    border-radius: 6px;
    min-width: 32px;
    min-height: 20px;
    padding: 2px 0px;
    margin-top: 6px;
}

.power-card:hover .power-card-key {
    color: #e6e2cc;
    background-color: rgba(139, 164, 176, 0.22);
    border-color: rgba(139, 164, 176, 0.35);
}

.power-card.danger .power-card-key {
    color: #c4746e;
    background-color: rgba(196, 116, 110, 0.12);
    border: 1px solid rgba(196, 116, 110, 0.22);
}

.power-card.danger:hover .power-card-key {
    color: #e6e2cc;
    background-color: rgba(196, 116, 110, 0.25);
    border-color: rgba(196, 116, 110, 0.45);
}

.power-cancel-btn {
    min-width: 140px;
    min-height: 36px;
    font-size: 13px;
    border-radius: 10px;
    margin-top: 8px;
    transition: all 0.2s ease;
}

.power-cancel-btn:hover {
    background-color: #2d2a2a;
    color: #e6e2cc;
    border-color: rgba(197, 201, 197, 0.25);
}

/* Wallpaper Gallery */
.wall-card {
    min-width: 140px;
    border-radius: 12px;
    background-color: #221f1f;
    border: 2px solid rgba(197, 201, 197, 0.08);
    padding: 6px;
    transition: all 0.2s ease;
}
.wall-card:hover {
    background-color: #2a2727;
    border-color: rgba(135, 169, 135, 0.4);
}
.wall-card.active {
    border-color: #87a987;
    background-color: rgba(135, 169, 135, 0.12);
}
.wall-thumb {
    border-radius: 8px;
}
.wall-caption {
    font-size: 10px;
    font-weight: 600;
    color: #a6a69c;
    margin-top: 4px;
}
.wall-card.active .wall-caption {
    color: #e6e2cc;
}

/* Media Player Card & Controls */
.media-badge {
    font-size: 11px;
    font-weight: 600;
    color: #87a987;
    background-color: rgba(135, 169, 135, 0.12);
    border: 1px solid rgba(135, 169, 135, 0.25);
    border-radius: 8px;
    padding: 3px 8px;
}
.media-status-pill {
    font-size: 10.5px;
    font-weight: 600;
    color: #87a987;
    background-color: rgba(135, 169, 135, 0.16);
    border-radius: 8px;
    padding: 3px 8px;
}
.media-status-pill.paused {
    color: #a6a69c;
    background-color: rgba(197, 201, 197, 0.08);
}
.media-card {
    background-color: #221f1f;
    border: 1px solid rgba(197, 201, 197, 0.08);
    border-radius: 14px;
    padding: 10px 12px;
}
.media-art-box {
    min-width: 64px;
    min-height: 64px;
    border-radius: 10px;
    background-color: rgba(197, 201, 197, 0.05);
    border: 1px solid rgba(197, 201, 197, 0.08);
}
.media-art {
    border-radius: 10px;
}
.media-art-fallback {
    font-size: 26px;
    color: #87a987;
}
.media-title {
    font-size: 13px;
    font-weight: 700;
    color: #e6e2cc;
}
.media-artist {
    font-size: 11px;
    color: #8a8980;
}
.media-progress {
    margin-top: 4px;
    margin-bottom: 2px;
}
.media-progress trough {
    min-height: 5px;
    border-radius: 3px;
    background-color: rgba(197, 201, 197, 0.12);
    border: none;
}
.media-progress progress {
    min-height: 5px;
    border-radius: 3px;
    background-color: #87a987;
}
.media-time {
    font-size: 10px;
    color: #737c73;
    font-family: monospace;
}
.media-btn {
    min-width: 34px;
    min-height: 34px;
    border-radius: 10px;
    border: none;
    background-color: rgba(197, 201, 197, 0.06);
    color: #c5c9c5;
    font-size: 15px;
}
.media-btn:hover {
    background-color: rgba(197, 201, 197, 0.15);
    color: #e6e2cc;
}
.play-btn {
    min-width: 40px;
    min-height: 40px;
    border-radius: 20px;
    border: none;
    background-color: #87a987;
    color: #181616;
    font-size: 16px;
    font-weight: 700;
}
.play-btn:hover {
    background-color: #98bb98;
    color: #181616;
}

/* Thin Scrollbar */
scrollbar {
    background: transparent;
    border: none;
}
scrollbar slider {
    min-width: 4px;
    min-height: 4px;
    border-radius: 4px;
    background-color: rgba(197, 201, 197, 0.15);
}
scrollbar slider:hover {
    background-color: rgba(135, 169, 135, 0.4);
}
"#;
