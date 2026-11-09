//! Cloud tab of the settings dialog: Cloudflare R2 and Google Drive.
//! Non-secret fields go to config.json; keys and tokens go to the keychain.

use crate::main_view::MainView;
use capshot_cloud::credentials::{self as creds, Keychain, SecretStore};
use capshot_cloud::gdrive::{GDrive, AUTH_TIMEOUT};
use capshot_cloud::r2::R2;
use capshot_core::config::{GDriveConfig, R2Config};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme, Disableable, Sizable};
use gpui_kit::*;

pub struct CloudSettings {
    main: WeakEntity<MainView>,
    account: Entity<InputState>,
    bucket: Entity<InputState>,
    public_url: Entity<InputState>,
    directory: Entity<InputState>,
    access_key: Entity<InputState>,
    secret_key: Entity<InputState>,
    client_id: Entity<InputState>,
    client_secret: Entity<InputState>,
    folder: Entity<InputState>,
    r2_status: SharedString,
    drive_status: SharedString,
    busy: bool,
}

fn field(
    window: &mut Window,
    cx: &mut Context<CloudSettings>,
    value: &str,
    placeholder: &str,
    masked: bool,
) -> Entity<InputState> {
    let (value, placeholder) = (value.to_string(), placeholder.to_string());
    cx.new(|cx| InputState::new(window, cx).default_value(value).placeholder(placeholder).masked(masked))
}

impl CloudSettings {
    pub fn new(
        main: WeakEntity<MainView>,
        r2: &R2Config,
        drive: &GDriveConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let has = |k: &str| matches!(Keychain.get(k), Ok(Some(v)) if !v.is_empty());
        let saved = |k: &str| if has(k) { "Saved in keychain (type to replace)" } else { "" };
        let this = Self {
            account: field(window, cx, &r2.account_id, "Account ID", false),
            bucket: field(window, cx, &r2.bucket, "Bucket name", false),
            public_url: field(window, cx, &r2.public_url, "https://pub-xxxx.r2.dev or custom domain", false),
            directory: field(window, cx, &r2.directory, "Folder inside the bucket (optional)", false),
            access_key: field(window, cx, "", saved(creds::R2_ACCESS_KEY_ID), true),
            secret_key: field(window, cx, "", saved(creds::R2_SECRET_ACCESS_KEY), true),
            client_id: field(window, cx, "", saved(creds::GDRIVE_CLIENT_ID), false),
            client_secret: field(window, cx, "", saved(creds::GDRIVE_CLIENT_SECRET), true),
            folder: field(window, cx, &drive.folder_id, "Drive folder ID (optional)", false),
            r2_status: "".into(),
            drive_status: if GDrive::new(&Keychain, "").is_connected() {
                "Signed in".into()
            } else {
                "Not signed in".into()
            },
            busy: false,
            main,
        };
        this
    }

    fn value(e: &Entity<InputState>, cx: &App) -> String {
        e.read(cx).value().trim().to_string()
    }

    fn r2_config(&self, cx: &App) -> R2Config {
        R2Config {
            account_id: Self::value(&self.account, cx),
            bucket: Self::value(&self.bucket, cx),
            public_url: Self::value(&self.public_url, cx),
            directory: Self::value(&self.directory, cx),
        }
    }

    /// Write fields to config and secrets to the keychain. Empty secret fields
    /// keep the stored value.
    pub fn save(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let r2 = self.r2_config(cx);
        let folder = Self::value(&self.folder, cx);
        for (key, input) in [
            (creds::R2_ACCESS_KEY_ID, &self.access_key),
            (creds::R2_SECRET_ACCESS_KEY, &self.secret_key),
            (creds::GDRIVE_CLIENT_ID, &self.client_id),
            (creds::GDRIVE_CLIENT_SECRET, &self.client_secret),
        ] {
            let v = Self::value(input, cx);
            if !v.is_empty() {
                Keychain.set(key, &v).map_err(|e| e.to_string())?;
            }
        }
        if let Some(main) = self.main.upgrade() {
            main.update(cx, |m, cx| {
                m.config.cloud.r2 = r2;
                m.config.cloud.gdrive.folder_id = folder;
                m.save_config();
                m.refresh_cloud_state();
                cx.notify();
            });
        }
        Ok(())
    }

    fn run<T: Send + 'static>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        job: impl FnOnce() -> Result<T, String> + Send + 'static,
        done: impl FnOnce(&mut Self, Result<T, String>) + 'static,
    ) {
        self.busy = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_spawn(async move { job() }).await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                done(this, result);
                if let Some(m) = this.main.upgrade() {
                    m.update(cx, |m, _| m.refresh_cloud_state());
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn test_r2(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(e) = self.save(cx) {
            self.r2_status = e.into();
            return;
        }
        let cfg = self.r2_config(cx);
        self.r2_status = "Testing…".into();
        self.run(
            window,
            cx,
            move || R2::from_store(&cfg, &Keychain).and_then(|r| r.test_connection()).map_err(|e| e.to_string()),
            |this, r| {
                this.r2_status = match r {
                    Ok(()) => "Connected to the bucket".into(),
                    Err(e) => e.into(),
                };
            },
        );
    }

    fn clear_r2(&mut self, cx: &mut Context<Self>) {
        let _ = Keychain.delete(creds::R2_ACCESS_KEY_ID);
        let _ = Keychain.delete(creds::R2_SECRET_ACCESS_KEY);
        self.r2_status = "R2 keys removed".into();
        if let Some(m) = self.main.upgrade() {
            m.update(cx, |m, _| m.refresh_cloud_state());
        }
        cx.notify();
    }

    fn connect_drive(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(e) = self.save(cx) {
            self.drive_status = e.into();
            return;
        }
        let session = match GDrive::new(&Keychain, "").start_auth() {
            Ok(s) => s,
            Err(e) => {
                self.drive_status = e.to_string().into();
                cx.notify();
                return;
            }
        };
        cx.open_url(&session.url);
        self.drive_status = "Waiting for the browser…".into();
        self.run(
            window,
            cx,
            move || {
                let g = GDrive::new(&Keychain, "");
                g.finish_auth(session, AUTH_TIMEOUT).and_then(|_| g.account_email()).map_err(|e| e.to_string())
            },
            |this, r| {
                this.drive_status = match r {
                    Ok(email) => format!("Signed in as {email}").into(),
                    Err(e) => e.into(),
                };
            },
        );
    }

    /// Revoking talks to Google, so it runs off the UI thread.
    fn disconnect_drive(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.drive_status = "Signing out…".into();
        self.run(
            window,
            cx,
            || GDrive::new(&Keychain, "").disconnect().map_err(|e| e.to_string()),
            |this, r| {
                this.drive_status = match r {
                    Ok(()) => "Signed out".into(),
                    Err(e) => e.into(),
                };
            },
        );
    }
}

fn labeled(label: &'static str, input: &Entity<InputState>) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .py_1()
        .child(div().w(px(120.)).text_xs().child(label))
        .child(div().flex_1().child(Input::new(input).small()))
}

impl Render for CloudSettings {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let busy = self.busy;
        div()
            .id("cloud-settings")
            .max_h(px(380.))
            .overflow_y_scroll()
            .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child("Cloudflare R2"))
            .child(labeled("Account ID", &self.account))
            .child(labeled("Bucket", &self.bucket))
            .child(labeled("Public URL", &self.public_url))
            .child(labeled("Folder", &self.directory))
            .child(labeled("Access key ID", &self.access_key))
            .child(labeled("Secret key", &self.secret_key))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .py_1()
                    .child(
                        Button::new("r2-test")
                            .label("Save & test")
                            .small()
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, w, cx| this.test_r2(w, cx))),
                    )
                    .child(
                        Button::new("r2-clear")
                            .label("Remove keys")
                            .small()
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| this.clear_r2(cx))),
                    )
                    .child(div().text_xs().text_color(muted).child(self.r2_status.clone())),
            )
            .child(div().mt_3().text_sm().font_weight(FontWeight::SEMIBOLD).child("Google Drive"))
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("Use your own OAuth client (type: Desktop app) from Google Cloud Console."),
            )
            .child(labeled("Client ID", &self.client_id))
            .child(labeled("Client secret", &self.client_secret))
            .child(labeled("Folder ID", &self.folder))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .py_1()
                    .child(
                        Button::new("drive-connect")
                            .label("Save & sign in")
                            .small()
                            .primary()
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, w, cx| this.connect_drive(w, cx))),
                    )
                    .child(
                        Button::new("drive-disconnect")
                            .label("Sign out")
                            .small()
                            .ghost()
                            .on_click(cx.listener(|this, _, w, cx| this.disconnect_drive(w, cx))),
                    )
                    .child(div().text_xs().text_color(muted).child(self.drive_status.clone())),
            )
    }
}
