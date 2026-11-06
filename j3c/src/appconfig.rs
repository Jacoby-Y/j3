//! `j3.appconfig`: the project's `[splash]` section and `[dist] icon`, baked into the program as constants
//! (made in memory by the loader, like `j3.i18nconfig`), so a built binary needs no j3.toml.
//! `j3.app` reads it: `App.run` shows the splash (`j3.game.splash`) and sets the window icon.

use std::fs;
use std::path::Path;

pub const MODULE: &str = "j3.appconfig";
pub const LABEL: &str = "<j3.appconfig>";

pub fn config_module(root: &Path) -> String {
    let text = fs::read_to_string(root.join("j3.toml")).unwrap_or_default();
    let get = |k: &str| crate::project::toml_str(&text, "splash", k);
    let on = |k: &str, d: bool| get(k).map_or(d, |v| v == "true");
    let cards = crate::project::toml_list(&text, "splash", "cards").unwrap_or_default();
    let card_size = get("card_size").and_then(|v| v.parse::<f64>().ok()).map_or(0.35, |f| f.clamp(0.05, 1.0));
    let card_ms = get("card_time").and_then(|v| v.parse::<f64>().ok()).map_or(1800, |s| (s * 1000.0) as i64);
    // The window icon loads PNGs only; an .svg `[dist] icon` still goes into packages, and the window keeps the logo.
    let icon = crate::project::toml_str(&text, "dist", "icon").filter(|p| p.to_ascii_lowercase().ends_with(".png")).unwrap_or_default();
    let q = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "/// `[splash]` and `[dist] icon` of j3.toml, baked in at build time.\nmodule j3.appconfig;\n\
pub const SPLASH: bool = {};\npub const LOGO: bool = {};\npub const LIGHT: bool = {};\npub const SKIPPABLE: bool = {};\n\
pub const CARD_MS: int = {};\npub const CARD_SIZE: f32 = {:?};\npub const CARDS: string = \"{}\";\npub const ICON: string = \"{}\";\n",
        on("enabled", false),
        on("logo", true),
        get("theme").as_deref() == Some("light"),
        on("skippable", true),
        card_ms.max(0),
        card_size,
        q(&cards.join("\n")).replace('\n', "\\n"),
        q(&icon),
    )
}
