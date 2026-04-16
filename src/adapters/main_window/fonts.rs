use std::fs;

use eframe::egui;
use tracing::{info, warn};

pub fn setup_chinese_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let Some((font_name, bytes)) = load_cjk_font_bytes() else {
        warn!("no CJK font file found, keep egui default fonts");
        return;
    };

    fonts
        .font_data
        .insert(font_name.clone(), egui::FontData::from_owned(bytes).into());

    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, font_name.clone());

    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .push(font_name.clone());

    ctx.set_fonts(fonts);
    info!("loaded CJK font: {}", font_name);
}

fn load_cjk_font_bytes() -> Option<(String, Vec<u8>)> {
    let candidates = [
        r"C:\Windows\Fonts\NotoSansSC-VF.ttf",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\Deng.ttf",
        r"C:\Windows\Fonts\msyh.ttc",
    ];

    for path in candidates {
        let Ok(bytes) = fs::read(path) else {
            continue;
        };
        let name = std::path::Path::new(path)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("cjk-font")
            .to_string();
        return Some((name, bytes));
    }

    None
}
