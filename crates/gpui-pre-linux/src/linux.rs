mod dispatcher;
mod display_connection;
mod headless;
mod keyboard;
mod platform;
mod system_notifications;
#[cfg(any(feature = "wayland", feature = "x11"))]
mod text_system;
#[cfg(feature = "wayland")]
mod wayland;
#[cfg(feature = "x11")]
mod x11;

#[cfg(any(feature = "wayland", feature = "x11"))]
mod xdg_desktop_portal;

pub use dispatcher::*;
pub(crate) use display_connection::{Backend, DisplayConnection, select_backend};
pub(crate) use headless::*;
pub(crate) use keyboard::*;
pub(crate) use platform::*;
#[cfg(any(feature = "wayland", feature = "x11"))]
pub(crate) use text_system::*;
#[cfg(feature = "wayland")]
pub(crate) use wayland::*;
#[cfg(feature = "x11")]
pub(crate) use x11::*;

use std::cell::RefCell;
use std::rc::Rc;

use gpui::WindowingModes;

thread_local! {
    static NEXT_ACTIVATION_TOKEN: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Makes the next window activation hand `token` to the compositor, in place of a token the app
/// asks for itself, which the compositor is likely to refuse. The `XDG_ACTIVATION_TOKEN` another
/// launch of the app was started with proves the user asked for the window. Only Wayland reads
/// it.
// TODO: a side channel, since `PlatformWindow::activate` is in gpui, which this fork does not
// carry. Goes when `activate` takes the token.
pub fn activate_next_window_with_token(token: String) {
    NEXT_ACTIVATION_TOKEN.set(Some(token));
}

#[cfg(feature = "wayland")]
pub(crate) fn take_next_activation_token() -> Option<String> {
    NEXT_ACTIVATION_TOKEN.take()
}

/// Returns the default platform implementation for the current OS.
///
/// A windowed platform connects to the display server the process environment names, or starts
/// headless when it names none, and can switch between those modes later. A headless platform
/// stays headless.
pub fn current_platform(headless: bool) -> Rc<dyn gpui::Platform> {
    if headless || std::env::var_os("ZED_HEADLESS").is_some() {
        linux_platform(WindowingModes::HEADLESS)
    } else {
        linux_platform(WindowingModes::all())
    }
}

/// Returns a platform that may switch among `allowed_modes`.
///
/// It starts windowed in the process's own environment, or headless if that names no allowed
/// display server. Set another initial mode with [`gpui::Application::with_windowing`].
pub fn linux_platform(allowed_modes: WindowingModes) -> Rc<dyn gpui::Platform> {
    Rc::new(LinuxPlatform::new(allowed_modes))
}
