use gpui::{App, Pixels, px};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ActiveTheme as _;

/// How popup menus are sized and shaped, for every menu the app opens,
/// including the ones components open on their own.
///
/// The defaults draw menus as the kit always has.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[non_exhaustive]
pub struct MenuSettings {
    /// The space between a menu's edges and its items, default 4px.
    pub padding: Pixels,
    /// The height of an item in a menu of the default size, default 26px.
    /// A small menu's items stay 20px.
    pub item_height: Pixels,
    /// The corner radius of an item, or `None` for the theme's radius, up to
    /// 8px. Zero draws square items.
    pub item_radius: Option<Pixels>,
    /// The corner radius of a menu, or `None` for the theme's radius.
    pub radius: Option<Pixels>,
    /// Whether a separator stops where the items do, rather than running
    /// through the padding to the menu's edges, default false.
    pub inset_separators: bool,
}

impl Default for MenuSettings {
    fn default() -> Self {
        Self {
            padding: px(4.),
            item_height: px(26.),
            item_radius: None,
            radius: None,
            inset_separators: false,
        }
    }
}

impl MenuSettings {
    /// The corner radius of an item in a menu of the default size.
    pub fn resolved_item_radius(&self, cx: &App) -> Pixels {
        self.item_radius
            .unwrap_or_else(|| cx.theme().radius.min(px(8.)))
    }

    /// The corner radius of a menu.
    pub fn resolved_radius(&self, cx: &App) -> Pixels {
        self.radius.unwrap_or(cx.theme().radius)
    }
}
