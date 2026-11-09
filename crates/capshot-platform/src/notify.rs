//! Desktop notifications ("Screenshot saved").

pub fn show(title: &str, body: &str) {
    let mut n = notify_rust::Notification::new();
    n.summary(title).body(body).appname("capshot");
    if let Err(e) = n.show() {
        log::warn!("notification failed: {e}");
    }
}
