use crate::i18n::{t, tfmt};
use gtk4::{prelude::*, Align, Box as GtkBox, Button, Label, Orientation};

/// The logo the icon set is made from, so the page shows the real app icon.
const LOGO_PNG: &[u8] = include_bytes!("../../media/shotbolt-logo-400.png");

#[cfg(not(target_os = "windows"))]
fn open_in_browser(url: &'static str) {
    std::thread::spawn(move || {
        let _ = crate::utils::open::open_url(url);
    });
}

pub struct AboutSettingsWidgets {
    pub section: GtkBox,
}

fn logo() -> gtk4::Widget {
    let bytes = gtk4::glib::Bytes::from_static(LOGO_PNG);
    match gtk4::gdk::Texture::from_bytes(&bytes) {
        Ok(texture) => {
            let picture = gtk4::Picture::for_paintable(&texture);
            picture.set_can_shrink(true);
            picture.set_size_request(128, 128);
            picture.set_halign(Align::Center);
            picture.set_margin_bottom(16);
            picture.upcast()
        }
        Err(_) => GtkBox::new(Orientation::Vertical, 0).upcast(),
    }
}

pub fn build_about_section() -> AboutSettingsWidgets {
    let section = GtkBox::new(Orientation::Vertical, 0);
    section.set_hexpand(true);
    section.set_vexpand(true);
    section.set_halign(Align::Center);
    section.set_valign(Align::Center);
    section.set_width_request(400);

    // --- 1. ICON & TITLE ---
    let header_vbox = GtkBox::new(Orientation::Vertical, 8);

    let title = Label::new(Some("Shotbolt"));
    title.add_css_class("about-app-name");

    let version = Label::new(Some(&tfmt(
        "Version {version}",
        &[("version", env!("CARGO_PKG_VERSION"))],
    )));
    version.add_css_class("about-version-label");

    header_vbox.append(&logo());
    header_vbox.append(&title);
    header_vbox.append(&version);
    section.append(&header_vbox);

    // --- 2. UPDATE ACTION ---
    let update_vbox = GtkBox::new(Orientation::Vertical, 12);
    update_vbox.set_margin_top(40);

    let check_btn = Button::with_label(&t("Check for Updates"));
    check_btn.add_css_class("settings-primary-btn");
    check_btn.set_width_request(200);
    check_btn.set_halign(Align::Center);

    let update_status = Label::new(None);
    update_status.add_css_class("settings-sub-option-hint");
    update_status.set_wrap(true);
    update_status.set_justify(gtk4::Justification::Center);
    update_status.set_max_width_chars(40);
    update_status.set_halign(Align::Center);
    update_status.set_visible(false);

    wire_update_check(&check_btn, &update_status);

    update_vbox.append(&check_btn);
    update_vbox.append(&update_status);
    section.append(&update_vbox);

    AboutSettingsWidgets { section }
}

/// Ask Shotbolt's update server on click and say what it found; a newer
/// release opens the update window.
#[cfg(target_os = "windows")]
fn wire_update_check(button: &Button, status: &Label) {
    let status = status.clone();
    button.connect_clicked(move |button| {
        button.set_sensitive(false);
        status.remove_css_class("settings-toast-error");
        status.set_text(&t("Checking for updates…"));
        status.set_visible(true);

        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(crate::update::check_now());
        });

        let button = button.clone();
        let status = status.clone();
        gtk4::glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            match receiver.try_recv() {
                Ok(Ok(Some(release))) => {
                    status.set_text(&tfmt(
                        "Shotbolt {version} is available.",
                        &[("version", release.version.as_str())],
                    ));
                    crate::update_ui::open_update_window(&release);
                }
                Ok(Ok(None)) => status.set_text(&t("Shotbolt is up to date.")),
                Ok(Err(error)) => {
                    eprintln!("[update] manual check failed: {error}");
                    status.set_text(&t("Could not reach the update server."));
                    status.add_css_class("settings-toast-error");
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    return gtk4::glib::ControlFlow::Continue;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    status.set_text(&t("Could not reach the update server."));
                }
            }
            button.set_sensitive(true);
            gtk4::glib::ControlFlow::Break
        });
    });
}

#[cfg(not(target_os = "windows"))]
fn wire_update_check(button: &Button, _status: &Label) {
    button.connect_clicked(|_| open_in_browser("https://github.com/keinbug/shotbolt/releases"));
}
