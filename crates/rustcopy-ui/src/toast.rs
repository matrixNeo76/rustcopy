//! The system notification shown when a copy ends while the window is not in front (GUI Slint plan,
//! phase 5d, CATALOGO_COMPORTAMENTI_GUI.md L30).
//!
//! Windows shows a toast from a desktop program only if the program has an identity, an
//! AppUserModelID, that the system knows: the installer creates the Start menu shortcut carrying it and
//! the registry entry that gives it a name. Without them (a build run from `target\release`) the
//! notification is silently dropped by Windows, which is why a failure here is never an error: the
//! taskbar button still flashes and the lavoro shows its outcome.

/// The identity of the console. **Stable**: the installer writes the same string into the Start menu
/// shortcut and the registry, and changing it would orphan both.
pub const APP_USER_MODEL_ID: &str = "rustcopy.console";

/// Text safe to put between XML tags and inside attributes.
pub fn escape_xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Control characters other than tab/newline are not allowed in XML 1.0 at all.
            c if c.is_control() && c != '\n' && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

/// The toast's content: a title and one line of text, no buttons and no sound of its own.
pub fn toast_xml(title: &str, body: &str) -> String {
    format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
        escape_xml(title),
        escape_xml(body)
    )
}

/// Tells Windows which identity this process has, so its windows and toasts group under it. Called
/// once, before the first window exists.
#[cfg(windows)]
pub fn set_process_identity() {
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

    let wide: Vec<u16> = std::ffi::OsStr::new(APP_USER_MODEL_ID)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: `wide` is a valid NUL-terminated UTF-16 string that outlives the call; the API copies it.
    // Failure only means the window groups under the executable's default identity.
    unsafe {
        let _ = SetCurrentProcessExplicitAppUserModelID(wide.as_ptr());
    }
}

#[cfg(not(windows))]
pub fn set_process_identity() {}

/// Shows a toast on a thread of its own, so a slow or refused notification never delays the window.
/// Every failure is swallowed: see the module note.
#[cfg(windows)]
pub fn show(title: &str, body: &str) {
    let xml = toast_xml(title, body);
    std::thread::spawn(move || {
        let _ = show_now(&xml);
    });
}

#[cfg(windows)]
fn show_now(xml: &str) -> windows::core::Result<()> {
    use windows::core::HSTRING;
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};

    // SAFETY: plain initialisation of the Windows Runtime for this thread; "already initialised"
    // answers are harmless and ignored.
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }
    let document = XmlDocument::new()?;
    document.LoadXml(&HSTRING::from(xml))?;
    let toast = ToastNotification::CreateToastNotification(&document)?;
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_USER_MODEL_ID))?
        .Show(&toast)
}

#[cfg(not(windows))]
pub fn show(_title: &str, _body: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_characters_are_escaped_and_controls_dropped() {
        assert_eq!(
            escape_xml("a&b <c> \"d\" 'e'\u{7}x"),
            "a&amp;b &lt;c&gt; &quot;d&quot; &apos;e&apos;x"
        );
    }

    #[test]
    fn the_toast_xml_carries_both_lines_and_cannot_be_broken_out_of() {
        let xml = toast_xml("rustcopy", "foto</text><text>evil");
        assert!(xml.starts_with("<toast><visual><binding template=\"ToastGeneric\">"));
        assert!(xml.contains("<text>rustcopy</text>"));
        assert!(xml.contains("foto&lt;/text&gt;&lt;text&gt;evil"));
        assert_eq!(xml.matches("<text>").count(), 2, "{xml}");
    }
}
